//! `<data dir>/ui/preferences.json`.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use pmhv_model::Preferences;

#[derive(Debug, thiserror::Error)]
pub enum PreferencesError {
    #[error("failed to read {path}")]
    Read { path: PathBuf, source: io::Error },
    #[error("invalid preferences file {path}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to write {path}")]
    Write { path: PathBuf, source: io::Error },
}

pub struct PreferencesStore {
    path: PathBuf,
    current: Mutex<Preferences>,
}

impl PreferencesStore {
    pub fn load(data_dir: &Path) -> Result<Self, PreferencesError> {
        let path = data_dir.join("ui").join("preferences.json");
        let current = match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).map_err(|source| PreferencesError::Parse {
                path: path.clone(),
                source,
            })?,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Preferences::default(),
            Err(source) => return Err(PreferencesError::Read { path, source }),
        };
        Ok(Self {
            path,
            current: Mutex::new(current),
        })
    }

    pub fn get(&self) -> Preferences {
        self.lock().clone()
    }

    pub fn set(&self, preferences: Preferences) -> Result<(), PreferencesError> {
        let write_error = |source| PreferencesError::Write {
            path: self.path.clone(),
            source,
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(write_error)?;
        }
        let json =
            serde_json::to_vec_pretty(&preferences).map_err(|source| PreferencesError::Parse {
                path: self.path.clone(),
                source,
            })?;
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, json)
            .and_then(|()| std::fs::rename(&temporary, &self.path))
            .map_err(write_error)?;
        *self.lock() = preferences;
        Ok(())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Preferences> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
