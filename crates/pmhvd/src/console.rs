//! Bridges a browser WebSocket to a console's unix socket, byte for byte.

use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tracing::debug;

const CHUNK_BYTES: usize = 16 * 1024;

pub async fn pipe(socket: WebSocket, stream: UnixStream) {
    let (mut socket_tx, mut socket_rx) = socket.split();
    let (mut read, mut write) = stream.into_split();

    let to_browser = async {
        let mut buffer = vec![0; CHUNK_BYTES];
        loop {
            let count = match read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let message = Message::Binary(buffer[..count].to_vec().into());
            if socket_tx.send(message).await.is_err() {
                break;
            }
        }
        // Tell the browser the console went away (the machine stopped).
        let _ = socket_tx.send(Message::Close(None)).await;
    };
    let from_browser = async {
        while let Some(Ok(message)) = socket_rx.next().await {
            let bytes = match message {
                Message::Binary(bytes) => bytes.to_vec(),
                Message::Text(text) => text.as_bytes().to_vec(),
                Message::Close(_) => break,
                Message::Ping(_) | Message::Pong(_) => continue,
            };
            if write.write_all(&bytes).await.is_err() {
                break;
            }
        }
    };

    tokio::select! {
        () = to_browser => {}
        () = from_browser => {}
    }
    debug!("console connection closed");
}
