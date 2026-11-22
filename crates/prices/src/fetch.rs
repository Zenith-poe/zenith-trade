//! Downloads the overview of one league.

use std::fmt;
use std::time::Duration;

use ureq::Agent;
use ureq::http::HeaderMap;

/// Where the overview is asked for. The one place that names the source, so
/// that it can point at a mirror later.
pub const SOURCE: &str = "https://poe.ninja/poe1/api/economy/current/dense/overviews";

/// Upper bound on one whole exchange, from connecting to the last byte read.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Wait reported for a 429 that says nothing about how long to wait.
const REFUSAL_WAIT_S: u32 = 60;

/// Largest body accepted. An overview is about 3.3 MB today.
const BODY_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    /// The source said to wait.
    RateLimited { wait_s: u32 },
    /// The source answered with a browser check instead of data.
    Challenge,
    /// Any other status that is not a success, a redirect included.
    Http(u16),
    /// The request did not complete, or the answer could not be read.
    Network(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RateLimited { wait_s } => write!(f, "rate limited: wait {wait_s} s"),
            Self::Challenge => write!(f, "the price source asks for a browser check"),
            Self::Http(status) => write!(f, "the price source answered with status {status}"),
            Self::Network(reason) => write!(f, "network error: {reason}"),
        }
    }
}

impl std::error::Error for FetchError {}

/// Gets the overview of `league` and returns its body as received: one
/// request, never tried again here.
///
/// `source` is [`SOURCE`] in the application and a local server in tests;
/// `user_agent` is sent as it is.
pub fn fetch(source: &str, league: &str, user_agent: &str) -> Result<String, FetchError> {
    let agent: Agent = Agent::config_builder()
        .user_agent(user_agent)
        // Statuses are read here, not turned into errors by the library.
        .http_status_as_error(false)
        // A redirect is never followed: requests go to one host only.
        .max_redirects(0)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into();
    let mut response = agent
        .get(source)
        .query("league", league)
        .query("language", "en")
        .call()
        .map_err(|error| FetchError::Network(error.to_string()))?;

    let status = response.status().as_u16();
    if status == 429 {
        let wait_s = header(response.headers(), "retry-after")
            .and_then(|text| text.trim().parse::<u32>().ok())
            .unwrap_or(REFUSAL_WAIT_S);
        return Err(FetchError::RateLimited { wait_s });
    }
    let is_page = header(response.headers(), "content-type").is_some_and(|kind| {
        kind.trim_start()
            .to_ascii_lowercase()
            .starts_with("text/html")
    });
    if status == 403 && is_page {
        return Err(FetchError::Challenge);
    }
    if !(200..300).contains(&status) {
        return Err(FetchError::Http(status));
    }
    response
        .body_mut()
        .with_config()
        .limit(BODY_LIMIT)
        .read_to_string()
        .map_err(|error| FetchError::Network(error.to_string()))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok()
}
