//! Who may use the HTTP API. Browsers need a session cookie; the CLI's local socket is
//! protected by filesystem permissions instead and skips all of this.

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::api::{ApiFailure, AppState};
use crate::auth::SessionKind;

pub const SESSION_COOKIE: &str = "pmhv_session";

/// Paths anyone may call: signing in, and asking whether you're signed in.
const PUBLIC: &[&str] = &["/api/v1/auth/login", "/api/v1/auth/session"];
/// Paths a setup session (temporary password) may call.
const SETUP: &[&str] = &["/api/v1/auth/password", "/api/v1/auth/logout"];

pub fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|cookies| cookies.split(';'))
        .filter_map(|cookie| cookie.trim().split_once('='))
        .find(|(name, _)| *name == SESSION_COOKIE)
        .map(|(_, value)| value.to_owned())
}

pub fn session_cookie(token: &str, max_age_secs: u64) -> HeaderValue {
    let cookie = format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs}"
    );
    HeaderValue::from_str(&cookie).expect("session tokens are hex, so the cookie is a valid header")
}

pub async fn require_session(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    if !path.starts_with("/api/") || PUBLIC.contains(&path) {
        return next.run(request).await;
    }
    let session = session_token(request.headers()).and_then(|token| state.auth.session(&token));
    match session {
        Some(SessionKind::Full) => next.run(request).await,
        Some(SessionKind::Setup) if SETUP.contains(&path) => next.run(request).await,
        Some(SessionKind::Setup) => ApiFailure::new(
            StatusCode::FORBIDDEN,
            "setup_required",
            "set a password before using pmhv",
        )
        .into_response(),
        None => ApiFailure::new(StatusCode::UNAUTHORIZED, "unauthenticated", "sign in first")
            .into_response(),
    }
}

/// Rejects requests that change things, and WebSocket upgrades, when they come from a page on
/// another origin. Browsers always send `Origin` for these, so its absence means a non-browser
/// client, which a session cookie would have to be stolen to use anyway.
pub async fn same_origin(request: Request<Body>, next: Next) -> Response {
    let is_upgrade = request.headers().contains_key(header::UPGRADE);
    let changes_state = !matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if (changes_state || is_upgrade) && !origin_matches_host(request.headers()) {
        return ApiFailure::new(
            StatusCode::FORBIDDEN,
            "cross_origin",
            "requests from other sites aren't allowed",
        )
        .into_response();
    }
    next.run(request).await
}

fn origin_matches_host(headers: &HeaderMap) -> bool {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return true;
    };
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    let origin_host = origin.split_once("://").map(|(_, rest)| rest);
    origin_host.is_some() && origin_host == host
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(pairs: &[(header::HeaderName, &str)]) -> HeaderMap {
        pairs
            .iter()
            .map(|(name, value)| (name.clone(), HeaderValue::from_str(value).expect("header")))
            .collect()
    }

    #[test]
    fn finds_the_session_cookie() {
        let cases = [
            ("pmhv_session=abc", Some("abc")),
            ("theme=dark; pmhv_session=abc; other=1", Some("abc")),
            ("pmhv_sessions=abc", None),
            ("", None),
        ];
        for (cookie, expected) in cases {
            let headers = headers(&[(header::COOKIE, cookie)]);
            assert_eq!(session_token(&headers).as_deref(), expected, "{cookie:?}");
        }
    }

    #[test]
    fn compares_origin_with_host() {
        let cases = [
            (Some("http://localhost:8640"), Some("localhost:8640"), true),
            (Some("http://evil.example"), Some("localhost:8640"), false),
            (Some("http://localhost:5173"), Some("localhost:8640"), false),
            (Some("null"), Some("localhost:8640"), false),
            (None, Some("localhost:8640"), true),
        ];
        for (origin, host, expected) in cases {
            let mut pairs = Vec::new();
            if let Some(origin) = origin {
                pairs.push((header::ORIGIN, origin));
            }
            if let Some(host) = host {
                pairs.push((header::HOST, host));
            }
            assert_eq!(
                origin_matches_host(&headers(&pairs)),
                expected,
                "{origin:?} vs {host:?}"
            );
        }
    }
}
