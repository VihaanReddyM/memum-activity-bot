/// Hypixel API client with built-in caching and rate limiting.
///
/// The client wraps `reqwest::Client` and adds:
/// - A `TimedCache` so repeated lookups for the same UUID within ~30 seconds
///   return cached results without hitting the API.
/// - A `tokio::sync::Semaphore` that limits concurrent requests to avoid
///   exceeding Hypixel's rate limits (~120 requests/minute).
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use reqwest::Client;
use tokio::sync::Semaphore;
use tracing::{debug};
use std::collections::HashMap;

use super::models::{BedwarsStats, HypixelPlayerResponse, MojangProfile, PlayerData};
use crate::shared::cache::TimedCache;

/// Default cache TTL for Hypixel stat lookups (30 seconds).
const CACHE_TTL_SECS: u64 = 60;

/// Maximum number of concurrent Hypixel API requests.
const MAX_CONCURRENT_REQUESTS: usize = 2;

/// The Hypixel API client.
pub struct HypixelClient {
    /// Underlying HTTP client (connection pooling handled internally).
    http: Client,

    /// Hypixel API key sent in the `API-Key` header.
    api_key: String,

    /// TTL cache keyed by Minecraft UUID.
    cache: TimedCache<String, PlayerData>,

    /// Semaphore used to limit concurrent outgoing requests.
    rate_limiter: Semaphore,
}

impl HypixelClient {
    /// Create a new client with the given API key.
    pub fn new(api_key: String) -> Self {
        Self {
            http: Client::new(),
            api_key,
            cache: TimedCache::new(Duration::from_secs(CACHE_TTL_SECS)),
            rate_limiter: Semaphore::new(MAX_CONCURRENT_REQUESTS),
        }
    }

    // ---------------------------------------------------------------------
    // Mojang username -> UUID
    // ---------------------------------------------------------------------

    /// Resolve a Minecraft username to a UUID via the Mojang API.
    ///
    /// Returns the UUID as a dashless hex string.
    pub async fn resolve_username(&self, username: &str) -> Result<MojangProfile> {
        let url = format!(
            "https://api.mojang.com/users/profiles/minecraft/{}",
            username
        );

        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("Failed to contact Mojang API")?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            bail!("Minecraft user '{}' not found", username);
        }

        let profile: MojangProfile = resp
            .json()
            .await
            .context("Failed to parse Mojang API response")?;

        debug!(username, uuid = %profile.id, "Resolved Minecraft username");
        Ok(profile)
    }

    // ---------------------------------------------------------------------
    // Hypixel 
    // ---------------------------------------------------------------------

    pub async fn fetch_player(self: &Arc<Self>, uuid: &str) -> Result<PlayerData> {
        if let Some(cached) = self.cache.get(&uuid.to_string()).await {
            return Ok(cached);
        }
    
        // acquire a permit (no Result -> no `?`)
        let _permit = self.rate_limiter.acquire().await;
    
        let url = format!("https://api.hypixel.net/v2/player?uuid={}", uuid);
    
        let resp = self
            .http
            .get(&url)
            .header("API-Key", &self.api_key)
            .send()
            .await?;
    
        let data: HypixelPlayerResponse = resp.json().await?;
    
        let (bedwars, socials) = match data.player {
            Some(player) => {
                let bw = player
                    .stats
                    .and_then(|s| s.bedwars)
                    .map(|bw| BedwarsStats::from_raw(&bw))
                    .unwrap_or_else(BedwarsStats::empty);
    
                let socials = player
                    .social_media
                    .map(|s| s.links)
                    .unwrap_or_default();
    
                (bw, socials)
            }
            None => (BedwarsStats::empty(), HashMap::new()),
        };
    
        let result = PlayerData {
            bedwars,
            social_links: socials,
        };
    
        self.cache.insert(uuid.to_string(), result.clone()).await;
    
        Ok(result)
    }
    
}
