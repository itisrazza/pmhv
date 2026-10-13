//! `pmhv console`: the terminal attached to a machine's serial console. Connects to the shim's
//! console socket directly, since the CLI runs as the same user as pmhvd.

use std::io::{self, IsTerminal};
use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

/// Ctrl+], as in telnet.
const DETACH: u8 = 0x1d;

#[derive(Debug, thiserror::Error)]
pub enum ConsoleError {
    #[error("can't connect to the console at {path} (is the machine running?)")]
    Connect { path: PathBuf, source: io::Error },
    #[error("couldn't switch the terminal to raw mode")]
    RawMode(#[source] io::Error),
    #[error("console connection failed")]
    Io(#[from] io::Error),
}

/// Restores the terminal however the session ends.
struct RawMode;

impl RawMode {
    fn enable() -> Result<Self, ConsoleError> {
        crossterm::terminal::enable_raw_mode().map_err(ConsoleError::RawMode)?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

pub async fn attach(socket: &Path, name: &str) -> Result<(), ConsoleError> {
    let stream = UnixStream::connect(socket)
        .await
        .map_err(|source| ConsoleError::Connect {
            path: socket.to_owned(),
            source,
        })?;
    eprintln!("Connected to {name}'s console. Press Ctrl+] to detach.");
    // Piped input (scripts, tests) has no terminal to put in raw mode.
    let raw = if std::io::stdin().is_terminal() {
        Some(RawMode::enable()?)
    } else {
        None
    };
    let (mut from_machine, mut to_machine) = stream.into_split();

    let output = async {
        let mut stdout = tokio::io::stdout();
        let mut buffer = vec![0; 4096];
        loop {
            let count = from_machine.read(&mut buffer).await?;
            if count == 0 {
                return Ok::<_, io::Error>(());
            }
            stdout.write_all(&buffer[..count]).await?;
            stdout.flush().await?;
        }
    };
    let input = async {
        let mut stdin = tokio::io::stdin();
        let mut buffer = vec![0; 1024];
        loop {
            let count = stdin.read(&mut buffer).await?;
            if count == 0 {
                return Ok::<_, io::Error>(());
            }
            let chunk = &buffer[..count];
            if let Some(index) = chunk.iter().position(|&byte| byte == DETACH) {
                to_machine.write_all(&chunk[..index]).await?;
                return Ok(());
            }
            to_machine.write_all(chunk).await?;
        }
    };

    let result = tokio::select! {
        result = output => result,
        result = input => result,
    };
    drop(raw);
    eprintln!("\r\nDetached.");
    result.map_err(ConsoleError::Io)
}
