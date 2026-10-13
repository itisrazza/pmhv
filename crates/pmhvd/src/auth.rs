//! The single user's credentials and browser sessions.
//!
//! On first launch (or after `pmhv auth reset`) there's only a temporary password, written to
//! `initial-password.txt` in the data directory and printed to the log. Signing in with it gives
//! a setup session that can only set a real password.
//!
//! Sessions are saved to `sessions.json` so they survive a restart. Only a hash of each token
//! is stored, so the file can't be used to sign in.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use rand::RngExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

pub const MIN_PASSWORD_CHARS: usize = 10;
pub const SESSION_LIFETIME: Duration = Duration::from_hours(7 * 24);
/// No 0/O or 1/l/I, so a temporary password read off a screen can be typed back reliably.
const PASSWORD_ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("wrong password")]
    InvalidCredentials,
    #[error("not signed in")]
    NoSession,
    #[error("passwords need at least {MIN_PASSWORD_CHARS} characters")]
    PasswordTooShort,
    #[error("failed to read {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid credentials file {path}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to write {path}")]
    Write { path: PathBuf, source: io::Error },
    #[error("failed to hash password: {0}")]
    Hash(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    /// Signed in with the temporary password; may only set a password.
    Setup,
    Full,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Credentials {
    /// PHC-format Argon2id hashes.
    password: Option<String>,
    temporary_password: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct Session {
    kind: SessionKind,
    /// Seconds since the Unix epoch.
    expires_at: u64,
}

/// Sessions by SHA-256 of their token, hex-encoded.
type Sessions = HashMap<String, Session>;

pub struct Auth {
    credentials_file: PathBuf,
    initial_password_file: PathBuf,
    sessions_file: PathBuf,
    credentials: Mutex<Credentials>,
    sessions: Mutex<Sessions>,
}

impl Auth {
    /// Loads credentials, issuing a temporary password if there's no way to sign in yet.
    pub fn load(data_dir: &Path) -> Result<Self, AuthError> {
        let credentials_file = data_dir.join("auth.json");
        let credentials = match std::fs::read_to_string(&credentials_file) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| AuthError::Parse {
                path: credentials_file.clone(),
                source,
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Credentials::default(),
            Err(source) => {
                return Err(AuthError::Read {
                    path: credentials_file,
                    source,
                });
            }
        };
        let sessions_file = data_dir.join("sessions.json");
        let sessions = read_sessions(&sessions_file)?;
        let auth = Self {
            initial_password_file: data_dir.join("initial-password.txt"),
            credentials_file,
            sessions_file,
            credentials: Mutex::new(credentials),
            sessions: Mutex::new(sessions),
        };
        let needs_password = {
            let credentials = lock(&auth.credentials);
            credentials.password.is_none() && credentials.temporary_password.is_none()
        };
        if needs_password {
            auth.issue_temporary()?;
        }
        Ok(auth)
    }

    pub async fn login(&self, password: &str) -> Result<(String, SessionKind), AuthError> {
        let (hash, temporary) = {
            let credentials = lock(&self.credentials);
            (
                credentials.password.clone(),
                credentials.temporary_password.clone(),
            )
        };
        let password = password.to_owned();
        // Argon2 is deliberately slow; keep it off the async worker threads.
        let kind = tokio::task::spawn_blocking(move || {
            if hash.as_deref().is_some_and(|hash| verify(&password, hash)) {
                Some(SessionKind::Full)
            } else if temporary
                .as_deref()
                .is_some_and(|hash| verify(&password, hash))
            {
                Some(SessionKind::Setup)
            } else {
                None
            }
        })
        .await
        .map_err(|error| AuthError::Hash(error.to_string()))?;
        let Some(kind) = kind else {
            warn!("failed sign-in attempt");
            return Err(AuthError::InvalidCredentials);
        };
        let token = random_token();
        self.update_sessions(|sessions| {
            sessions.insert(
                token_hash(&token),
                Session {
                    kind,
                    expires_at: now_secs() + SESSION_LIFETIME.as_secs(),
                },
            );
        });
        info!(?kind, "signed in");
        Ok((token, kind))
    }

    pub fn session(&self, token: &str) -> Option<SessionKind> {
        let now = now_secs();
        lock(&self.sessions)
            .get(&token_hash(token))
            .filter(|session| session.expires_at > now)
            .map(|session| session.kind)
    }

    pub fn logout(&self, token: &str) {
        self.update_sessions(|sessions| {
            sessions.remove(&token_hash(token));
        });
    }

    /// Changes the sessions, drops expired ones, and saves them. Failing to save is logged rather
    /// than returned: the sessions still work until pmhvd restarts.
    fn update_sessions(&self, change: impl FnOnce(&mut Sessions)) {
        let mut sessions = lock(&self.sessions);
        change(&mut sessions);
        let now = now_secs();
        sessions.retain(|_, session| session.expires_at > now);
        let saved = serde_json::to_vec(&*sessions)
            .map_err(|error| error.to_string())
            .and_then(|json| {
                write_private(&self.sessions_file, &json).map_err(|error| error.to_string())
            });
        if let Err(error) = saved {
            warn!(path = %self.sessions_file.display(), error, "failed to save sessions");
        }
    }

    /// Sets the password from a signed-in session, which becomes the only session.
    pub async fn set_password(&self, token: &str, password: &str) -> Result<(), AuthError> {
        if self.session(token).is_none() {
            return Err(AuthError::NoSession);
        }
        if password.chars().count() < MIN_PASSWORD_CHARS {
            return Err(AuthError::PasswordTooShort);
        }
        let password = password.to_owned();
        let hash = tokio::task::spawn_blocking(move || hash(&password))
            .await
            .map_err(|error| AuthError::Hash(error.to_string()))??;
        {
            let mut credentials = lock(&self.credentials);
            credentials.password = Some(hash);
            credentials.temporary_password = None;
            self.save(&credentials)?;
        }
        if let Err(error) = remove_if_present(&self.initial_password_file) {
            warn!(path = %self.initial_password_file.display(), %error, "failed to remove the initial password file");
        }
        let current = token_hash(token);
        self.update_sessions(|sessions| {
            sessions.retain(|existing, _| *existing == current);
            if let Some(session) = sessions.get_mut(&current) {
                session.kind = SessionKind::Full;
            }
        });
        info!("password set");
        Ok(())
    }

    /// Forgets the password, signs everyone out and issues a new temporary password.
    pub fn reset(&self) -> Result<String, AuthError> {
        {
            let mut credentials = lock(&self.credentials);
            credentials.password = None;
            self.save(&credentials)?;
        }
        self.update_sessions(HashMap::clear);
        self.issue_temporary()
    }

    fn issue_temporary(&self) -> Result<String, AuthError> {
        let password = temporary_password();
        let hashed = hash(&password)?;
        {
            let mut credentials = lock(&self.credentials);
            credentials.temporary_password = Some(hashed);
            self.save(&credentials)?;
        }
        write_private(
            &self.initial_password_file,
            format!("{password}\n").as_bytes(),
        )?;
        warn!(
            password,
            file = %self.initial_password_file.display(),
            "no password set: sign in with this temporary password, then choose your own"
        );
        Ok(password)
    }

    fn save(&self, credentials: &Credentials) -> Result<(), AuthError> {
        let json = serde_json::to_vec_pretty(credentials).map_err(|source| AuthError::Parse {
            path: self.credentials_file.clone(),
            source,
        })?;
        write_private(&self.credentials_file, &json)
    }
}

fn hash(password: &str) -> Result<String, AuthError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|error| AuthError::Hash(error.to_string()))
}

fn verify(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

fn read_sessions(path: &Path) -> Result<Sessions, AuthError> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).map_err(|source| AuthError::Parse {
            path: path.to_owned(),
            source,
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Sessions::new()),
        Err(source) => Err(AuthError::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

fn token_hash(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut hex, byte| {
            // Writing to a String can't fail.
            let _ = write!(hex, "{byte:02x}");
            hex
        })
}

fn random_token() -> String {
    let bytes: [u8; 32] = rand::rng().random();
    hex(&bytes)
}

/// Four groups of five, e.g. `k7qmz-2xwrf-…`.
fn temporary_password() -> String {
    let mut rng = rand::rng();
    (0..4)
        .map(|_| {
            (0..5)
                .map(|_| {
                    char::from(PASSWORD_ALPHABET[rng.random_range(0..PASSWORD_ALPHABET.len())])
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// Writes a file only the current user can read, via a temporary file and rename.
fn write_private(path: &Path, contents: &[u8]) -> Result<(), AuthError> {
    let write_error = |source| AuthError::Write {
        path: path.to_owned(),
        source,
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(write_error)?;
    }
    let temporary = path.with_extension("tmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&temporary)
        .map_err(write_error)?;
    file.write_all(contents).map_err(write_error)?;
    std::fs::rename(&temporary, path).map_err(write_error)
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
        _ => Ok(()),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn temporary_from_file(data: &Path) -> String {
        std::fs::read_to_string(data.join("initial-password.txt"))
            .expect("initial password file")
            .trim()
            .to_owned()
    }

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path)
            .expect("metadata")
            .permissions()
            .mode()
            & 0o777
    }

    #[tokio::test]
    async fn first_launch_to_password_to_reset() {
        let data = tempfile::tempdir().expect("temp dir");
        let auth = Auth::load(data.path()).expect("load");
        let temporary = temporary_from_file(data.path());
        assert_eq!(temporary.len(), 23, "four groups of five: {temporary}");
        assert_eq!(mode(&data.path().join("initial-password.txt")), 0o600);
        assert_eq!(mode(&data.path().join("auth.json")), 0o600);

        assert!(matches!(
            auth.login("wrong").await,
            Err(AuthError::InvalidCredentials)
        ));
        let (setup, kind) = auth.login(&temporary).await.expect("temporary login");
        assert_eq!(kind, SessionKind::Setup);
        let (other, _) = auth.login(&temporary).await.expect("second session");

        assert!(matches!(
            auth.set_password(&setup, "short").await,
            Err(AuthError::PasswordTooShort)
        ));
        auth.set_password(&setup, "correct horse battery")
            .await
            .expect("set password");
        assert_eq!(
            auth.session(&setup),
            Some(SessionKind::Full),
            "setting a password upgrades the session"
        );
        assert_eq!(auth.session(&other), None, "and signs out the others");
        assert!(!data.path().join("initial-password.txt").exists());
        assert!(matches!(
            auth.login(&temporary).await,
            Err(AuthError::InvalidCredentials)
        ));

        let reloaded = Auth::load(data.path()).expect("reload");
        assert_eq!(
            reloaded.session(&setup),
            Some(SessionKind::Full),
            "sessions survive a restart"
        );
        let stored =
            std::fs::read_to_string(data.path().join("sessions.json")).expect("sessions file");
        assert!(!stored.contains(&setup), "only token hashes are stored");
        assert_eq!(mode(&data.path().join("sessions.json")), 0o600);
        let (_, kind) = reloaded
            .login("correct horse battery")
            .await
            .expect("password persists");
        assert_eq!(kind, SessionKind::Full);

        let reset = reloaded.reset().expect("reset");
        assert_eq!(reset, temporary_from_file(data.path()));
        assert!(matches!(
            reloaded.login("correct horse battery").await,
            Err(AuthError::InvalidCredentials)
        ));
        let (_, kind) = reloaded.login(&reset).await.expect("new temporary works");
        assert_eq!(kind, SessionKind::Setup);
    }

    #[tokio::test]
    async fn logout_ends_the_session() {
        let data = tempfile::tempdir().expect("temp dir");
        let auth = Auth::load(data.path()).expect("load");
        let (token, _) = auth
            .login(&temporary_from_file(data.path()))
            .await
            .expect("login");
        auth.logout(&token);
        assert_eq!(auth.session(&token), None);
        assert!(matches!(
            auth.set_password(&token, "long enough password").await,
            Err(AuthError::NoSession)
        ));
    }
}
