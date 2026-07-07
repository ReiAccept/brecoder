use crate::config::{CookiesFile, StreamConfig, StreamStatus};
use biliup::downloader::live::{
    BilibiliOptions, LiveCredentials, LiveOptions, LivePlugin, LiveRequest, LiveStatus,
};
use chrono::Local;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Combined result from checking a room, containing status info and
/// (if live) the stream URL + headers needed for recording.
pub struct RoomCheckResult {
    pub status: StreamStatus,
    pub stream_url: Option<String>,
    pub stream_headers: HashMap<String, String>,
    pub suffix: Option<String>,
}

/// Client for checking Bilibili live room status and fetching stream URLs.
///
/// Uses the `biliup` crate's Bilibili plugin which handles WBI signing,
/// CDN selection, and the newer play-info API endpoints.
pub struct BilibiliClient {
    client: reqwest::Client,
    cookie: Option<String>,
    plugin: biliup::downloader::live::Bilibili,
}

impl BilibiliClient {
    pub fn new(cookies: &CookiesFile) -> Result<Self, Box<dyn std::error::Error>> {
        let cookie = cookies.to_cookie_header();
        let client = reqwest::Client::builder()
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/120.0.0.0 Safari/537.36",
            )
            .timeout(std::time::Duration::from_secs(15))
            .build()?;

        Ok(Self {
            client,
            cookie: Some(cookie),
            plugin: biliup::downloader::live::Bilibili::new(),
        })
    }

    /// Check a room and return its live status along with stream info if live.
    ///
    /// This performs the full check in one API round-trip: it fetches room
    /// info, determines liveness, and if live also resolves the play URL.
    pub async fn check_room(&self, stream: &StreamConfig) -> Option<RoomCheckResult> {
        let url = format!("https://live.bilibili.com/{}", stream.room_id);

        debug!(
            "Checking room {} ({}) via biliup plugin",
            stream.name, stream.room_id
        );

        let request = LiveRequest {
            client: self.client.clone(),
            url,
            name: stream.name.clone(),
            options: LiveOptions {
                bilibili: BilibiliOptions {
                    qn: stream.quality as u32,
                    ..Default::default()
                },
                ..Default::default()
            },
            credentials: LiveCredentials {
                bilibili_cookie: self.cookie.clone(),
                ..Default::default()
            },
        };

        match self.plugin.check_stream(request).await {
            Ok(LiveStatus::Live { stream: live }) => {
                info!(
                    "[LIVE] {} (room {}) — title=\"{}\", suffix={}",
                    stream.name, stream.room_id, live.title, live.suffix
                );
                Some(RoomCheckResult {
                    status: StreamStatus {
                        name: stream.name.clone(),
                        platform: stream.platform.clone(),
                        room_id: stream.room_id,
                        streaming: true,
                        title: live.title,
                        last_checked: Local::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string(),
                    },
                    stream_url: Some(live.raw_stream_url),
                    stream_headers: live.stream_headers,
                    suffix: Some(live.suffix),
                })
            }
            Ok(LiveStatus::Offline) => {
                info!(
                    "[OFFLINE] {} (room {})",
                    stream.name, stream.room_id
                );
                Some(RoomCheckResult {
                    status: StreamStatus {
                        name: stream.name.clone(),
                        platform: stream.platform.clone(),
                        room_id: stream.room_id,
                        streaming: false,
                        title: String::new(),
                        last_checked: Local::now()
                            .format("%Y-%m-%d %H:%M:%S")
                            .to_string(),
                    },
                    stream_url: None,
                    stream_headers: HashMap::new(),
                    suffix: None,
                })
            }
            Err(e) => {
                warn!(
                    "Bilibili check failed for {} (room {}): {}",
                    stream.name, stream.room_id, e
                );
                None
            }
        }
    }
}
