//! Blocking network work for generating a learning path. Everything here runs
//! on GPUI's background executor; nothing touches UI state.

pub mod docs;
pub mod gemini;
pub mod links;
pub mod partial;
pub mod stream;

use std::time::Duration;

/// One HTTP agent per generation. Non-2xx responses are returned (not turned
/// into errors) so callers can read the API's error body.
pub fn http_agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(90)))
        .http_status_as_error(false)
        .user_agent(concat!("Nuevette/", env!("CARGO_PKG_VERSION")))
        .build()
        .into()
}

/// Removes `secret` from `message`, for errors that might echo a request URL.
fn redact(message: String, secret: &str) -> String {
    if secret.is_empty() {
        message
    } else {
        message.replace(secret, "***")
    }
}
