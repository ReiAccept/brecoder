use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level config file (config.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Polling interval in seconds
    pub interval: u64,
    /// Path to ffmpeg binary (defaults to "ffmpeg")
    #[serde(default = "default_ffmpeg_path")]
    pub ffmpeg_path: String,
    /// Streams to monitor
    pub streams: Vec<StreamConfig>,
}

fn default_ffmpeg_path() -> String {
    "ffmpeg".to_string()
}

/// A single stream entry in config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub platform: String,
    pub name: String,
    pub room_id: u64,
    /// Recorder tool to use (e.g. "ffmpeg")
    pub recoder: String,
    /// Base output directory for recordings
    pub path: String,
    /// Stream quality number (defaults to 10000 = original)
    #[serde(default = "default_quality")]
    pub quality: u64,
}

fn default_quality() -> u64 {
    10000
}

/// Top-level cookies file (cookies.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookiesFile {
    pub cookie_info: CookieInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieInfo {
    pub cookies: Vec<CookieEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    pub name: String,
    pub value: String,
}

/// Live status for a stream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamStatus {
    pub name: String,
    pub platform: String,
    pub room_id: u64,
    pub streaming: bool,
    pub title: String,
    pub last_checked: String,
}

impl AppConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: AppConfig = serde_json::from_str(&content)?;
        Ok(config)
    }
}

impl CookiesFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let cookies: CookiesFile = serde_json::from_str(&content)?;
        Ok(cookies)
    }

    /// Build a cookie header string from the stored cookies
    pub fn to_cookie_header(&self) -> String {
        self.cookie_info
            .cookies
            .iter()
            .map(|c| format!("{}={}", c.name, c.value))
            .collect::<Vec<_>>()
            .join("; ")
    }
}
