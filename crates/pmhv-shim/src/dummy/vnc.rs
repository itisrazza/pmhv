//! Just enough of an RFB 3.8 server for noVNC: no authentication, raw encoding, the client's
//! choice of 32-bit true-colour pixel format, and keyboard and pointer input ignored.

use std::io;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{debug, warn};

use super::frame::{Frame, HEIGHT, WIDTH};

/// How the client wants pixels laid out.
#[derive(Debug, Clone, Copy)]
struct PixelFormat {
    big_endian: bool,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
}

impl PixelFormat {
    /// The format announced in `ServerInit`: 32 bpp, depth 24, little-endian, RGB at 16/8/0.
    const SERVER: Self = Self {
        big_endian: false,
        red_shift: 16,
        green_shift: 8,
        blue_shift: 0,
    };

    fn encode(self, frame: &Frame) -> Vec<u8> {
        let (pixels, _) = frame.pixels.as_chunks::<3>();
        pixels
            .iter()
            .flat_map(|rgb| {
                let value = (u32::from(rgb[0]) << self.red_shift)
                    | (u32::from(rgb[1]) << self.green_shift)
                    | (u32::from(rgb[2]) << self.blue_shift);
                if self.big_endian {
                    value.to_be_bytes()
                } else {
                    value.to_le_bytes()
                }
            })
            .collect()
    }
}

pub async fn serve(listener: UnixListener, frames: watch::Receiver<Arc<Frame>>, name: String) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let frames = frames.clone();
                let name = name.clone();
                tokio::spawn(async move {
                    if let Err(error) = client(stream, frames, &name).await {
                        debug!(%error, "VNC client disconnected");
                    }
                });
            }
            Err(error) => warn!(%error, "accepting VNC client failed"),
        }
    }
}

async fn client(
    stream: UnixStream,
    mut frames: watch::Receiver<Arc<Frame>>,
    name: &str,
) -> io::Result<()> {
    let (mut read, mut write) = stream.into_split();

    // Handshake: version, security type None, ClientInit, ServerInit.
    write.write_all(b"RFB 003.008\n").await?;
    let mut version = [0; 12];
    read.read_exact(&mut version).await?;
    write.write_all(&[1, 1]).await?;
    let _chosen = read.read_u8().await?;
    write.write_u32(0).await?;
    let _shared = read.read_u8().await?;

    let mut init = Vec::new();
    init.extend_from_slice(&WIDTH.to_be_bytes());
    init.extend_from_slice(&HEIGHT.to_be_bytes());
    init.extend_from_slice(&[32, 24, 0, 1]);
    init.extend_from_slice(&255u16.to_be_bytes().repeat(3));
    init.extend_from_slice(&[16, 8, 0, 0, 0, 0]);
    init.extend_from_slice(&u32::try_from(name.len()).unwrap_or(0).to_be_bytes());
    init.extend_from_slice(name.as_bytes());
    write.write_all(&init).await?;

    let mut format = PixelFormat::SERVER;
    let mut sent_once = false;
    loop {
        match read.read_u8().await? {
            // SetPixelFormat
            0 => {
                let mut message = [0; 19];
                read.read_exact(&mut message).await?;
                let pixel = &message[3..];
                if pixel[0] != 32 || pixel[3] == 0 {
                    return Err(io::Error::other("only 32-bit true colour is supported"));
                }
                format = PixelFormat {
                    big_endian: pixel[2] != 0,
                    red_shift: pixel[10],
                    green_shift: pixel[11],
                    blue_shift: pixel[12],
                };
            }
            // SetEncodings: raw is always allowed, so the list doesn't matter.
            2 => {
                let _padding = read.read_u8().await?;
                let count = read.read_u16().await?;
                let mut encodings = vec![0; usize::from(count) * 4];
                read.read_exact(&mut encodings).await?;
            }
            // FramebufferUpdateRequest
            3 => {
                let mut request = [0; 9];
                read.read_exact(&mut request).await?;
                let incremental = request[0] != 0;
                // Incremental requests wait for the next frame instead of resending this one.
                if incremental && sent_once && frames.changed().await.is_err() {
                    return Ok(());
                }
                let frame = Arc::clone(&frames.borrow_and_update());
                let mut update = vec![0, 0];
                update.extend_from_slice(&1u16.to_be_bytes());
                update.extend_from_slice(&[0, 0, 0, 0]);
                update.extend_from_slice(&WIDTH.to_be_bytes());
                update.extend_from_slice(&HEIGHT.to_be_bytes());
                update.extend_from_slice(&0i32.to_be_bytes());
                update.extend_from_slice(&format.encode(&frame));
                write.write_all(&update).await?;
                sent_once = true;
            }
            // KeyEvent and PointerEvent: read and ignore.
            4 => {
                let mut event = [0; 7];
                read.read_exact(&mut event).await?;
            }
            5 => {
                let mut event = [0; 5];
                read.read_exact(&mut event).await?;
            }
            // ClientCutText
            6 => {
                let mut padding = [0; 3];
                read.read_exact(&mut padding).await?;
                let length = read.read_u32().await?;
                let mut text = vec![0; usize::try_from(length).unwrap_or(0)];
                read.read_exact(&mut text).await?;
            }
            other => {
                return Err(io::Error::other(format!("unsupported RFB message {other}")));
            }
        }
    }
}
