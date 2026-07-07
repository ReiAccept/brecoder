use crate::bilibili::BilibiliClient;
use crate::config::{AppConfig, StreamConfig, StreamStatus};
use crate::recorder::RecorderManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Shared application state, accessible from both the monitor and web server.
#[derive(Clone)]
pub struct AppState {
    pub statuses: Arc<RwLock<Vec<StreamStatus>>>,
    /// Tracks previous live state per room_id to detect transitions
    pub previous: Arc<RwLock<HashMap<u64, bool>>>,
    /// Manages ffmpeg recording processes
    pub recorder: RecorderManager,
}

impl AppState {
    pub fn new(ffmpeg_path: &str) -> Self {
        Self {
            statuses: Arc::new(RwLock::new(Vec::new())),
            previous: Arc::new(RwLock::new(HashMap::new())),
            recorder: RecorderManager::new(ffmpeg_path),
        }
    }
}

/// Run the monitoring loop. Checks all streams every `interval` seconds.
pub async fn run_monitor(
    config: AppConfig,
    cookies: crate::config::CookiesFile,
    state: AppState,
) {
    let client = match BilibiliClient::new(&cookies) {
        Ok(c) => c,
        Err(e) => {
            error!("Failed to create Bilibili client: {}. Monitor not started.", e);
            return;
        }
    };

    let interval = std::time::Duration::from_secs(config.interval);
    info!(
        "Monitor started — checking {} stream(s) every {} seconds",
        config.streams.len(),
        config.interval
    );

    // Run initial check immediately
    check_all_streams(&client, &config.streams, &state).await;

    // Then loop on the interval
    let mut tick = tokio::time::interval(interval);
    // Skip the first tick since we already did an immediate check
    tick.tick().await;

    loop {
        tick.tick().await;
        check_all_streams(&client, &config.streams, &state).await;
    }
}

async fn check_all_streams(client: &BilibiliClient, streams: &[StreamConfig], state: &AppState) {
    let mut new_statuses = Vec::new();

    for stream in streams {
        let result = match client.check_room(stream).await {
            Some(r) => r,
            None => {
                // API call failed entirely — skip this stream this round
                error!(
                    "Failed to check room for {} (room {}), skipping",
                    stream.name, stream.room_id
                );
                continue;
            }
        };

        // Detect state transition
        let mut prev = state.previous.write().await;
        let was_live = prev.get(&stream.room_id).copied().unwrap_or(false);

        if !was_live && result.status.streaming {
            info!(
                "TRANSITION: {} (room {}) just went LIVE — \"{}\"",
                stream.name, stream.room_id, result.status.title
            );

            // Drop the write lock before starting recording (async operations ahead)
            drop(prev);

            // biliup already resolved the stream URL during check_room — use it directly
            match &result.stream_url {
                Some(stream_url) => {
                    state
                        .recorder
                        .start(
                            stream.room_id,
                            stream_url,
                            &stream.path,
                            &result.status.title,
                            &result.stream_headers,
                            result.suffix.as_deref(),
                        )
                        .await;
                }
                None => {
                    error!(
                        "Room {} ({}) reported LIVE but no stream URL was returned. \
                         Recording NOT started.",
                        stream.name, stream.room_id
                    );
                }
            }

            // Re-acquire the write lock to update previous state
            prev = state.previous.write().await;
        } else if was_live && !result.status.streaming {
            info!(
                "TRANSITION: {} (room {}) just went OFFLINE",
                stream.name, stream.room_id
            );

            // Drop the write lock before stopping recording
            drop(prev);

            // Stop the recording
            if let Some(path) = state.recorder.stop(stream.room_id).await {
                info!(
                    "Recording saved for {}: {}",
                    stream.name, path
                );
            }

            // Re-acquire the write lock to update previous state
            prev = state.previous.write().await;
        }

        prev.insert(stream.room_id, result.status.streaming);
        drop(prev);

        new_statuses.push(result.status);
    }

    // Update shared state
    let mut current = state.statuses.write().await;
    *current = new_statuses;
}
