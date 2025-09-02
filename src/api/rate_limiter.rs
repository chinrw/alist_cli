//! Rate limiting functionality for API requests.

use std::{num::NonZeroU32, sync::LazyLock, time::Duration};

use anyhow::{Result, anyhow};
use governor::{
    Quota, RateLimiter,
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use reqwest::Client;

use crate::CONFIG;

/// Request timeout in seconds
const REQ_TIMEOUT: u64 = 5;

/// Global rate limiter instance
static TPS_RATE_LIMITER: LazyLock<RateLimiter<NotKeyed, InMemoryState, DefaultClock>> =
    LazyLock::new(|| {
        let quota = Quota::per_second(
            NonZeroU32::new(CONFIG.tpslimit).unwrap_or_else(|| NonZeroU32::new(1).unwrap()),
        );
        RateLimiter::direct(quota)
    });

/// Performs a rate-limited POST request with JSON payload.
///
/// # Arguments
///
/// * `client` - The HTTP client to use for the request
/// * `url` - The URL to send the request to
/// * `payload` - The payload to serialize as JSON
///
/// # Returns
///
/// The HTTP response if successful
///
/// # Errors
///
/// Returns an error if the rate limiter times out or the request fails
pub async fn rate_limited_request<T>(
    client: &Client,
    url: String,
    payload: T,
) -> Result<reqwest::Response>
where
    T: serde::Serialize,
{
    // Wait until we're allowed to make a request
    tokio::time::timeout(
        Duration::from_secs(REQ_TIMEOUT),
        TPS_RATE_LIMITER.until_ready(),
    )
    .await
    .map_err(|_| anyhow!("Rate limiter timeout"))?;

    // Now make the request
    let response = client
        .post(url)
        .timeout(Duration::from_secs(REQ_TIMEOUT))
        .json(&payload)
        .header("Authorization", &CONFIG.token)
        .header("Content-Type", "application/json")
        .send()
        .await?;

    Ok(response)
}

/// Performs a rate-limited GET request.
///
/// # Arguments
///
/// * `client` - The HTTP client to use for the request
/// * `url` - The URL to send the request to
///
/// # Returns
///
/// The HTTP response if successful
///
/// # Errors
///
/// Returns an error if the rate limiter times out or the request fails
pub async fn rate_limited_get(client: &Client, url: &str) -> Result<reqwest::Response> {
    // Wait until we're allowed to make a request
    tokio::time::timeout(
        Duration::from_secs(REQ_TIMEOUT),
        TPS_RATE_LIMITER.until_ready(),
    )
    .await
    .map_err(|_| anyhow!("Rate limiter timeout"))?;

    // Now make the request
    let response = client.get(url).send().await?;

    Ok(response)
}
