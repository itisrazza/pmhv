use std::io;
use std::path::PathBuf;

use http_body_util::{BodyExt, Empty};
use hyper::body::Bytes;
use hyper::header::{ACCEPT, HOST};
use hyper::{Request, StatusCode};
use hyper_util::rt::TokioIo;
use pmhv_model::ApiError;
use serde::de::DeserializeOwned;
use tokio::net::UnixStream;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("cannot connect to pmhvd at {socket} (is it running?)")]
    Connect { socket: PathBuf, source: io::Error },
    #[error("HTTP exchange with pmhvd failed")]
    Http(#[from] hyper::Error),
    #[error("failed to build request for {path}")]
    Request {
        path: String,
        source: hyper::http::Error,
    },
    #[error("pmhvd returned {status} for {path}: {} ({})", .error.message, .error.code)]
    Api {
        path: String,
        status: StatusCode,
        error: ApiError,
    },
    #[error("pmhvd returned {status} for {path} with an unexpected body: {body}")]
    UnexpectedResponse {
        path: String,
        status: StatusCode,
        body: String,
    },
    #[error("cannot decode pmhvd's response for {path}")]
    Decode {
        path: String,
        source: serde_json::Error,
    },
}

/// Talks to pmhvd over its control socket.
pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: PathBuf) -> Self {
        Self { socket }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ClientError> {
        let stream =
            UnixStream::connect(&self.socket)
                .await
                .map_err(|source| ClientError::Connect {
                    socket: self.socket.clone(),
                    source,
                })?;
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
        // Connection failures also surface through `send_request`, which is where they're reported.
        tokio::spawn(connection);

        let request = Request::get(path)
            .header(HOST, "pmhvd")
            .header(ACCEPT, "application/json")
            .body(Empty::<Bytes>::new())
            .map_err(|source| ClientError::Request {
                path: path.to_owned(),
                source,
            })?;
        let response = sender.send_request(request).await?;
        let status = response.status();
        let body = response.into_body().collect().await?.to_bytes();

        if !status.is_success() {
            return Err(match serde_json::from_slice::<ApiError>(&body) {
                Ok(error) => ClientError::Api {
                    path: path.to_owned(),
                    status,
                    error,
                },
                Err(_) => ClientError::UnexpectedResponse {
                    path: path.to_owned(),
                    status,
                    body: String::from_utf8_lossy(&body).into_owned(),
                },
            });
        }
        serde_json::from_slice(&body).map_err(|source| ClientError::Decode {
            path: path.to_owned(),
            source,
        })
    }
}
