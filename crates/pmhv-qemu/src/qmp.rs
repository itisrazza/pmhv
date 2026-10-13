//! A small client for the QEMU Machine Protocol: newline-delimited JSON over a unix socket.

use std::collections::HashMap;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use serde::Deserialize;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, oneshot};

#[derive(Debug, thiserror::Error)]
pub enum QmpError {
    #[error("failed to connect to QMP socket {path}")]
    Connect {
        path: std::path::PathBuf,
        source: io::Error,
    },
    #[error("QMP connection failed")]
    Io(#[from] io::Error),
    #[error("QMP connection closed")]
    Closed,
    #[error("unexpected QMP message: {0}")]
    Unexpected(String),
    #[error("invalid QMP message")]
    Decode(#[from] serde_json::Error),
    #[error("QMP command {command} failed: {description} ({class})")]
    Command {
        command: String,
        class: String,
        description: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct QmpEvent {
    pub name: String,
    pub data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Message {
    Return {
        #[serde(rename = "return")]
        value: Value,
        id: Option<u64>,
    },
    Error {
        error: ErrorBody,
        id: Option<u64>,
    },
    Event {
        event: String,
        #[serde(default)]
        data: Value,
    },
    Greeting {
        #[serde(rename = "QMP")]
        _qmp: Value,
    },
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    class: String,
    desc: String,
}

type Reply = Result<Value, (String, String)>;
type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Reply>>>>;

pub struct Qmp {
    writer: tokio::sync::Mutex<OwnedWriteHalf>,
    pending: Pending,
    next_id: AtomicU64,
}

impl Qmp {
    /// Connects, completes capability negotiation, and returns the client plus its event stream.
    /// The stream ends when the connection closes (normally because QEMU exited).
    pub async fn connect(
        path: &Path,
    ) -> Result<(Self, mpsc::UnboundedReceiver<QmpEvent>), QmpError> {
        let stream = UnixStream::connect(path)
            .await
            .map_err(|source| QmpError::Connect {
                path: path.to_owned(),
                source,
            })?;
        Self::from_stream(stream).await
    }

    /// Like [`Qmp::connect`], over an already-connected socket. QEMU serves one QMP client at a
    /// time, so a connection used to probe for readiness has to be the one that's kept.
    pub async fn from_stream(
        stream: UnixStream,
    ) -> Result<(Self, mpsc::UnboundedReceiver<QmpEvent>), QmpError> {
        let (read, write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();

        let greeting = lines.next_line().await?.ok_or(QmpError::Closed)?;
        if !matches!(serde_json::from_str(&greeting)?, Message::Greeting { .. }) {
            return Err(QmpError::Unexpected(greeting));
        }

        let pending: Pending = Arc::default();
        let (events_tx, events_rx) = mpsc::unbounded_channel();
        tokio::spawn(read_loop(lines, Arc::clone(&pending), events_tx));

        let qmp = Self {
            writer: tokio::sync::Mutex::new(write),
            pending,
            next_id: AtomicU64::new(0),
        };
        qmp.execute("qmp_capabilities", None).await?;
        Ok((qmp, events_rx))
    }

    pub async fn execute(
        &self,
        command: &str,
        arguments: Option<Value>,
    ) -> Result<Value, QmpError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (reply_tx, reply_rx) = oneshot::channel();
        lock(&self.pending).insert(id, reply_tx);

        let mut request = json!({ "execute": command, "id": id });
        if let Some(arguments) = arguments {
            request["arguments"] = arguments;
        }
        let mut line = serde_json::to_vec(&request)?;
        line.push(b'\n');
        let written = self.writer.lock().await.write_all(&line).await;
        if let Err(error) = written {
            lock(&self.pending).remove(&id);
            return Err(error.into());
        }

        match reply_rx.await {
            Ok(Ok(value)) => Ok(value),
            Ok(Err((class, description))) => Err(QmpError::Command {
                command: command.to_owned(),
                class,
                description,
            }),
            Err(_) => Err(QmpError::Closed),
        }
    }
}

async fn read_loop(
    mut lines: tokio::io::Lines<BufReader<OwnedReadHalf>>,
    pending: Pending,
    events: mpsc::UnboundedSender<QmpEvent>,
) {
    loop {
        let line = match lines.next_line().await {
            Ok(Some(line)) => line,
            Ok(None) => break,
            Err(error) => {
                tracing::warn!(%error, "QMP read failed");
                break;
            }
        };
        match serde_json::from_str::<Message>(&line) {
            Ok(Message::Return {
                value,
                id: Some(id),
            }) => reply(&pending, id, Ok(value)),
            Ok(Message::Error {
                error,
                id: Some(id),
            }) => {
                reply(&pending, id, Err((error.class, error.desc)));
            }
            Ok(Message::Event { event, data }) => {
                // The receiver going away just means nobody's listening for events any more.
                let _ = events.send(QmpEvent { name: event, data });
            }
            Ok(other) => tracing::warn!(message = ?other, "unexpected QMP message"),
            Err(error) => tracing::warn!(%error, line, "undecodable QMP message"),
        }
    }
    // Dropping the senders wakes any waiting `execute` calls with `Closed`.
    lock(&pending).clear();
}

fn reply(pending: &Pending, id: u64, value: Reply) {
    if let Some(sender) = lock(pending).remove(&id) {
        // The caller may have given up waiting; that's not an error here.
        let _ = sender.send(value);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
