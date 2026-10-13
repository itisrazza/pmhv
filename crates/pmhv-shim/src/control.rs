//! The shim's control socket.

use std::sync::Arc;

use pmhv_driver::{Request, Response, read_message, write_message};
use pmhv_model::report::error_chain;
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tracing::warn;

use crate::machine::Machine;

pub async fn serve(machine: Arc<Machine>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                let machine = Arc::clone(&machine);
                tokio::spawn(async move {
                    if let Err(error) = handle(&machine, stream).await {
                        warn!(error = error_chain(&error), "control request failed");
                    }
                });
            }
            Err(error) => warn!(%error, "accepting control connection failed"),
        }
    }
}

async fn handle(machine: &Machine, stream: UnixStream) -> Result<(), pmhv_driver::MessageError> {
    let (read, mut write) = stream.into_split();
    let mut read = BufReader::new(read);
    let Some(request) = read_message::<Request>(&mut read).await? else {
        return Ok(());
    };

    match request {
        Request::Status => {
            let status = machine.status.borrow().clone();
            write_message(&mut write, &Response::Status(status)).await
        }
        Request::Watch => {
            let mut status = machine.status.subscribe();
            loop {
                let current = status.borrow_and_update().clone();
                let exited = current.exited;
                if write_message(&mut write, &Response::Status(current))
                    .await
                    .is_err()
                {
                    // The watcher went away (pmhvd restarting, usually); nothing to report.
                    return Ok(());
                }
                if exited || status.changed().await.is_err() {
                    return Ok(());
                }
            }
        }
        Request::Power { action } => {
            let response = match machine.power(action).await {
                Ok(()) => Response::Ok,
                Err(error) => Response::Error {
                    message: error_chain(&error),
                },
            };
            write_message(&mut write, &response).await
        }
        Request::Screenshot { path } => {
            let response = match machine.screenshot(&path).await {
                Ok(()) => Response::Ok,
                Err(error) => Response::Error {
                    message: error_chain(&error),
                },
            };
            write_message(&mut write, &response).await
        }
    }
}
