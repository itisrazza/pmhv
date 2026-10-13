//! Newline-delimited JSON over the shim's control socket. Each connection sends one request.
//! `status` and `power` get one response; `watch` gets a stream of statuses, ending with one
//! where `exited` is true just before the shim exits.

use std::io;

use pmhv_model::MachineState;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Status,
    Watch,
    Power { action: ShimPower },
    Screenshot { path: std::path::PathBuf },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShimPower {
    Stop,
    ForceStop,
    Reset,
    Pause,
    Resume,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Status(ShimStatus),
    Ok,
    Error { message: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShimStatus {
    pub state: MachineState,
    /// Set when the machine failed to start or stopped abnormally.
    pub error: Option<String>,
    /// True on the last status before the shim exits.
    pub exited: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MessageError {
    #[error("connection failed")]
    Io(#[from] io::Error),
    #[error("invalid message")]
    Decode(#[from] serde_json::Error),
}

/// Reads one message, or `None` at end of stream.
pub async fn read_message<T: DeserializeOwned>(
    reader: &mut (impl AsyncBufRead + Unpin),
) -> Result<Option<T>, MessageError> {
    let mut line = String::new();
    if reader.read_line(&mut line).await? == 0 {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&line)?))
}

pub async fn write_message<T: Serialize>(
    writer: &mut (impl AsyncWrite + Unpin),
    message: &T,
) -> Result<(), MessageError> {
    let mut line = serde_json::to_vec(message)?;
    line.push(b'\n');
    writer.write_all(&line).await?;
    writer.flush().await?;
    Ok(())
}
