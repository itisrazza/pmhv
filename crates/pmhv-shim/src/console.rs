//! Fans QEMU's single serial connection out to any number of console clients, keeping
//! scrollback so a client that connects later sees what came before.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, warn};

const SCROLLBACK_BYTES: usize = 1024 * 1024;
const CHUNK_BYTES: usize = 4096;

pub struct Console {
    scrollback: Mutex<VecDeque<u8>>,
    output: broadcast::Sender<Arc<[u8]>>,
    input: mpsc::Sender<Vec<u8>>,
    input_rx: Mutex<Option<mpsc::Receiver<Vec<u8>>>>,
}

impl Console {
    pub fn new() -> Arc<Self> {
        let (output, _) = broadcast::channel(256);
        let (input, input_rx) = mpsc::channel(64);
        Arc::new(Self {
            scrollback: Mutex::new(VecDeque::new()),
            output,
            input,
            input_rx: Mutex::new(Some(input_rx)),
        })
    }

    /// Starts relaying QEMU's serial chardev. Input sent before this is queued.
    pub fn attach(self: &Arc<Self>, serial: UnixStream) {
        let Some(mut input_rx) = lock(&self.input_rx).take() else {
            warn!("serial console attached twice; ignoring the second connection");
            return;
        };
        let (mut read, mut write) = serial.into_split();

        tokio::spawn(async move {
            while let Some(bytes) = input_rx.recv().await {
                if let Err(error) = write.write_all(&bytes).await {
                    warn!(%error, "writing to serial console failed");
                    break;
                }
            }
        });

        let console = Arc::clone(self);
        tokio::spawn(async move {
            let mut buffer = vec![0; CHUNK_BYTES];
            loop {
                match read.read(&mut buffer).await {
                    Ok(0) => break,
                    Ok(count) => console.publish(&buffer[..count]),
                    Err(error) => {
                        warn!(%error, "reading serial console failed");
                        break;
                    }
                }
            }
        });
    }

    /// The last `bytes` of scrollback.
    pub fn tail(&self, bytes: usize) -> Vec<u8> {
        let scrollback = lock(&self.scrollback);
        let skip = scrollback.len().saturating_sub(bytes);
        scrollback.iter().skip(skip).copied().collect()
    }

    fn publish(&self, bytes: &[u8]) {
        // Holding the lock while broadcasting keeps subscribers from missing or repeating bytes.
        let mut scrollback = lock(&self.scrollback);
        scrollback.extend(bytes);
        let excess = scrollback.len().saturating_sub(SCROLLBACK_BYTES);
        scrollback.drain(..excess);
        // No subscribers is fine: the bytes are still in the scrollback.
        let _ = self.output.send(Arc::from(bytes));
    }

    fn subscribe(&self) -> (Vec<u8>, broadcast::Receiver<Arc<[u8]>>) {
        let scrollback = lock(&self.scrollback);
        (
            scrollback.iter().copied().collect(),
            self.output.subscribe(),
        )
    }

    pub async fn serve(self: Arc<Self>, listener: UnixListener) {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(Arc::clone(&self).serve_client(stream));
                }
                Err(error) => warn!(%error, "accepting console client failed"),
            }
        }
    }

    async fn serve_client(self: Arc<Self>, stream: UnixStream) {
        let (mut read, mut write) = stream.into_split();
        let (snapshot, mut output) = self.subscribe();

        let input = self.input.clone();
        let reader = tokio::spawn(async move {
            let mut buffer = vec![0; CHUNK_BYTES];
            while let Ok(count) = read.read(&mut buffer).await {
                if count == 0 || input.send(buffer[..count].to_vec()).await.is_err() {
                    break;
                }
            }
        });

        if write.write_all(&snapshot).await.is_ok() {
            loop {
                match output.recv().await {
                    Ok(bytes) => {
                        if write.write_all(&bytes).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "console client fell behind; output skipped");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
        reader.abort();
        debug!("console client disconnected");
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
