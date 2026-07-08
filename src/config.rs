use serde::{Deserialize, Serialize};
use std::path::Path;

/// Top-level config file (config.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// Server bind address and port
    #[serde(default)]
    pub server: ServerConfig,
    /// Polling interval in seconds
    pub interval: u64,
    /// Path to ffmpeg binary (defaults to "ffmpeg")
    #[serde(default = "default_ffmpeg_path")]
    pub ffmpeg_path: String,
    /// Streams to monitor
    #[serde(default)]
    pub streamers: Vec<StreamConfig>,
    /// Upload configuration (if present, enables upload after recording)
    #[serde(default)]
    pub upload: Option<UploadConfig>,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_addr")]
    pub addr: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: default_addr(),
            port: default_port(),
        }
    }
}

fn default_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    3000
}

fn default_ffmpeg_path() -> String {
    "ffmpeg".to_string()
}

/// Upload configuration for Bilibili submission
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadConfig {
    /// Title template with variables: {streamer.name}, {stream.title}, plus chrono format specifiers
    pub title: String,
    /// 1 = original, 2 = reprint
    #[serde(default = "default_copyright")]
    pub copyright: u8,
    /// Source URL template (for reprint)
    #[serde(default)]
    pub source: String,
    /// Category/TID for submission
    pub tid: u16,
    /// Tags for the video
    #[serde(default)]
    pub tags: Vec<String>,
    /// Video description template
    #[serde(default)]
    pub desc: String,
    /// Dynamic/feed post template
    #[serde(default)]
    pub dynamic: String,
}

fn default_copyright() -> u8 {
    1
}

/// A single stream/streamer entry in config.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub platform: String,
    pub name: String,
    pub room_id: u64,
    /// Recorder tool to use (e.g. "ffmpeg")
    #[serde(alias = "recoder")]
    pub recorder: String,
    /// Base output directory for recordings
    pub path: String,
    /// Stream quality number (defaults to 10000 = original)
    #[serde(default = "default_quality")]
    pub quality: u64,
    /// Whether to upload after recording stops
    #[serde(default)]
    pub upload: bool,
    /// Output format override (e.g. "flv", "mp4")
    #[serde(default)]
    pub format: Option<String>,
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
