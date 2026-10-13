//! Installs pmhvd as a per-user background service: a launchd agent on macOS, a systemd user
//! unit on Linux. Either way, stopping or restarting pmhvd must leave the shims (and so the
//! machines) running, which both service managers would otherwise prevent by killing everything
//! pmhvd started.

use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

pub const LAUNCHD_LABEL: &str = "io.razza.pmhvd";
pub const SYSTEMD_UNIT: &str = "pmhvd.service";

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("pmhvd not found at {0}; pass --pmhvd with its path")]
    DaemonNotFound(PathBuf),
    #[error("can't find the home directory")]
    NoHome,
    #[error("can't find pmhv's own location")]
    CurrentExe(#[source] io::Error),
    #[error("installing a service isn't supported on {0}")]
    Unsupported(&'static str),
    #[error("failed to write {path}")]
    Write { path: PathBuf, source: io::Error },
    #[error("failed to remove {path}")]
    Remove { path: PathBuf, source: io::Error },
    #[error("failed to run {program}")]
    Run { program: String, source: io::Error },
    #[error("{program} failed: {stderr}")]
    Failed { program: String, stderr: String },
}

/// Everything a service definition needs.
#[derive(Debug, Clone)]
pub struct ServiceSpec {
    pub daemon: PathBuf,
    /// Captured from the installing shell, since service managers start with a minimal PATH and
    /// pmhvd needs to find QEMU.
    pub path_env: String,
    pub log: PathBuf,
}

pub fn default_daemon() -> Result<PathBuf, ServiceError> {
    Ok(std::env::current_exe()
        .map_err(ServiceError::CurrentExe)?
        .with_file_name("pmhvd"))
}

impl ServiceSpec {
    pub fn new(daemon: PathBuf) -> Result<Self, ServiceError> {
        if !daemon.is_file() {
            return Err(ServiceError::DaemonNotFound(daemon));
        }
        let home = dirs_home()?;
        let log = if cfg!(target_os = "macos") {
            home.join("Library/Logs/pmhv/pmhvd.log")
        } else {
            home.join(".local/state/pmhv/pmhvd.log")
        };
        Ok(Self {
            daemon,
            path_env: std::env::var("PATH").unwrap_or_default(),
            log,
        })
    }
}

fn dirs_home() -> Result<PathBuf, ServiceError> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or(ServiceError::NoHome)
}

/// Where the definition goes, and its contents.
pub fn definition(spec: &ServiceSpec) -> Result<(PathBuf, String), ServiceError> {
    let home = dirs_home()?;
    match std::env::consts::OS {
        "macos" => Ok((
            home.join("Library/LaunchAgents")
                .join(format!("{LAUNCHD_LABEL}.plist")),
            launchd_plist(spec),
        )),
        "linux" => Ok((
            home.join(".config/systemd/user").join(SYSTEMD_UNIT),
            systemd_unit(spec),
        )),
        other => Err(ServiceError::Unsupported(other)),
    }
}

pub fn install(spec: &ServiceSpec) -> Result<PathBuf, ServiceError> {
    let (path, contents) = definition(spec)?;
    for dir in [path.parent(), spec.log.parent()].into_iter().flatten() {
        std::fs::create_dir_all(dir).map_err(|source| ServiceError::Write {
            path: dir.to_owned(),
            source,
        })?;
    }
    std::fs::write(&path, contents).map_err(|source| ServiceError::Write {
        path: path.clone(),
        source,
    })?;
    if cfg!(target_os = "macos") {
        // Replacing an existing agent: unload the old one first, ignoring "not loaded".
        let _ = run("launchctl", &["bootout", &launchd_target()?]);
        run(
            "launchctl",
            &["bootstrap", &launchd_domain()?, &path.to_string_lossy()],
        )?;
    } else {
        run("systemctl", &["--user", "daemon-reload"])?;
        run("systemctl", &["--user", "enable", "--now", SYSTEMD_UNIT])?;
    }
    Ok(path)
}

pub fn uninstall() -> Result<PathBuf, ServiceError> {
    let home = dirs_home()?;
    let path = if cfg!(target_os = "macos") {
        run("launchctl", &["bootout", &launchd_target()?])?;
        home.join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"))
    } else {
        run("systemctl", &["--user", "disable", "--now", SYSTEMD_UNIT])?;
        home.join(".config/systemd/user").join(SYSTEMD_UNIT)
    };
    std::fs::remove_file(&path).map_err(|source| ServiceError::Remove {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

fn launchd_domain() -> Result<String, ServiceError> {
    Ok(format!("gui/{}", current_uid()?))
}

fn launchd_target() -> Result<String, ServiceError> {
    Ok(format!("{}/{LAUNCHD_LABEL}", launchd_domain()?))
}

/// The home directory belongs to the current user; reading its owner avoids an unsafe
/// `getuid()` call.
fn current_uid() -> Result<u32, ServiceError> {
    use std::os::unix::fs::MetadataExt;
    let home = dirs_home()?;
    std::fs::metadata(&home)
        .map(|metadata| metadata.uid())
        .map_err(|source| ServiceError::Run {
            program: format!("stat {}", home.display()),
            source,
        })
}

fn run(program: &str, args: &[&str]) -> Result<(), ServiceError> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|source| ServiceError::Run {
            program: program.to_owned(),
            source,
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(ServiceError::Failed {
            program: format!("{program} {}", args.join(" ")),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        })
    }
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn launchd_plist(spec: &ServiceSpec) -> String {
    let daemon = xml_escape(&spec.daemon.to_string_lossy());
    let log = xml_escape(&spec.log.to_string_lossy());
    let path_env = xml_escape(&spec.path_env);
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{LAUNCHD_LABEL}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{daemon}</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
		<key>PATH</key>
		<string>{path_env}</string>
	</dict>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<true/>
	<!-- Machines run under pmhv-shim, which must outlive pmhvd restarts. -->
	<key>AbandonProcessGroup</key>
	<true/>
	<key>StandardOutPath</key>
	<string>{log}</string>
	<key>StandardErrorPath</key>
	<string>{log}</string>
</dict>
</plist>
"#
    )
}

fn systemd_unit(spec: &ServiceSpec) -> String {
    let daemon = systemd_quote(&spec.daemon);
    let log = spec.log.to_string_lossy();
    format!(
        r#"[Unit]
Description=pmhv daemon

[Service]
ExecStart={daemon}
Environment="PATH={path_env}"
Restart=on-failure
# Machines run under pmhv-shim, which must outlive pmhvd restarts.
KillMode=process
StandardOutput=append:{log}
StandardError=append:{log}

[Install]
WantedBy=default.target
"#,
        path_env = spec.path_env,
    )
}

fn systemd_quote(path: &Path) -> String {
    let text = path.to_string_lossy();
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> ServiceSpec {
        ServiceSpec {
            daemon: PathBuf::from("/Applications/pmhv & co/pmhvd"),
            path_env: "/opt/homebrew/bin:/usr/bin".to_owned(),
            log: PathBuf::from("/Users/me/Library/Logs/pmhv/pmhvd.log"),
        }
    }

    #[test]
    fn launchd_agent_keeps_shims_alive() {
        let plist = launchd_plist(&spec());
        let cases = [
            "<string>io.razza.pmhvd</string>",
            "<string>/Applications/pmhv &amp; co/pmhvd</string>",
            "<key>AbandonProcessGroup</key>\n\t<true/>",
            "<string>/opt/homebrew/bin:/usr/bin</string>",
        ];
        for expected in cases {
            assert!(
                plist.contains(expected),
                "missing {expected:?} in:\n{plist}"
            );
        }
    }

    #[test]
    fn systemd_unit_keeps_shims_alive() {
        let unit = systemd_unit(&spec());
        let cases = [
            "ExecStart=\"/Applications/pmhv & co/pmhvd\"",
            "KillMode=process",
            "Environment=\"PATH=/opt/homebrew/bin:/usr/bin\"",
            "WantedBy=default.target",
        ];
        for expected in cases {
            assert!(unit.contains(expected), "missing {expected:?} in:\n{unit}");
        }
    }
}
