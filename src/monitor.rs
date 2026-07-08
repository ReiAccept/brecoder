use crate::bilibili::BilibiliClient;
use crate::config::{AppConfig, StreamConfig, StreamStatus, UploadConfig};
use crate::recorder::RecorderManager;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

/// Shared application state, accessible from both the monitor and web server.
#[derive(Clone)]
pub struct AppState {
    pub statuses: Arc<RwLock<Vec<StreamStatus>>>,
    /// Tracks previous live state per room_id to detect transitions
    pub previous: Arc<RwLock<HashMap<u64, bool>>>,
    /// Manages ffmpeg recording processes
    pub recorder: RecorderManager,
    /// Path to the cookies file (for upload authentication)
    pub cookies_path: Arc<String>,
    /// Global upload configuration from config.json
    pub upload_config: Arc<Option<UploadConfig>>,
}

impl AppState {
    pub fn new(ffmpeg_path: &str, cookies_path: String, upload_config: Option<UploadConfig>) -> Self {
        Self {
            statuses: Arc::new(RwLock::new(Vec::new())),
            previous: Arc::new(RwLock::new(HashMap::new())),
            recorder: RecorderManager::new(ffmpeg_path),
            cookies_path: Arc::new(cookies_path),
            upload_config: Arc::new(upload_config),
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
        config.streamers.len(),
        config.interval
    );

    // Run initial check immediately
    check_all_streams(&client, &config.streamers, &state).await;

    // Then loop on the interval
    let mut tick = tokio::time::interval(interval);
    // Skip the first tick since we already did an immediate check
    tick.tick().await;

    loop {
        tick.tick().await;
        check_all_streams(&client, &config.streamers, &state).await;
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
                            // Use config.format if set, otherwise use the suffix from the plugin
                            stream
                                .format
                                .as_deref()
                                .or(result.suffix.as_deref()),
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
            let stream_title = result.status.title.clone();

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

                // Trigger upload if enabled for this streamer
                if stream.upload {
                    if let Some(ref upload_cfg) = *state.upload_config {
                        let cookies_path = state.cookies_path.as_ref().clone();
                        let stream_cfg = stream.clone();
                        let upload_cfg_clone = upload_cfg.clone();

                        info!(
                            "Scheduling upload for {}: {}",
                            stream.name, path
                        );

                        tokio::spawn(async move {
                            match crate::uploader::upload_video(
                                &cookies_path,
                                &stream_cfg,
                                &upload_cfg_clone,
                                &path,
                                &stream_title,
                            )
                            .await
                            {
                                Ok(()) => {
                                    info!(
                                        "Upload completed for {}: {}",
                                        stream_cfg.name, path
                                    );
                                }
                                Err(e) => {
                                    error!(
                                        "Upload failed for {} ({}): {}",
                                        stream_cfg.name, path, e
                                    );
                                }
                            }
                        });
                    } else {
                        warn!(
                            "Upload enabled for {} but no [upload] section in config.json — skipping upload",
                            stream.name
                        );
                    }
                }
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
