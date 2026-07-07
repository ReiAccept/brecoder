use chrono::Local;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::{Child, Command};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Manages ffmpeg recording processes keyed by room_id.
#[derive(Clone)]
pub struct RecorderManager {
    processes: Arc<RwLock<HashMap<u64, RecorderHandle>>>,
    ffmpeg_path: String,
}

struct RecorderHandle {
    child: Child,
    output_path: String,
}

impl RecorderManager {
    pub fn new(ffmpeg_path: &str) -> Self {
        Self {
            processes: Arc::new(RwLock::new(HashMap::new())),
            ffmpeg_path: ffmpeg_path.to_string(),
        }
    }

    /// Check if a room is currently being recorded.
    pub async fn is_recording(&self, room_id: u64) -> bool {
        self.processes.read().await.contains_key(&room_id)
    }

    /// Get the output path of an active recording, if any.
    pub async fn recording_path(&self, room_id: u64) -> Option<String> {
        self.processes
            .read()
            .await
            .get(&room_id)
            .map(|h| h.output_path.clone())
    }

    /// Start recording a stream to disk.
    ///
    /// `stream_url` — direct stream URL from the platform API.
    /// `output_dir` — base directory from config (timestamp + ext appended automatically).
    /// `stream_title` — used in the filename for identification.
    /// `stream_headers` — HTTP headers (referer, cookie, user-agent, etc.) for the stream request.
    /// `suffix` — file extension override ("flv" or "m3u8").
    pub async fn start(
        &self,
        room_id: u64,
        stream_url: &str,
        output_dir: &str,
        stream_title: &str,
        stream_headers: &HashMap<String, String>,
        suffix: Option<&str>,
    ) {
        // Prevent duplicate recordings
        if self.is_recording(room_id).await {
            warn!(
                "Room {} is already being recorded at {}",
                room_id,
                self.recording_path(room_id).await.unwrap_or_default()
            );
            return;
        }

        // Determine file extension from suffix or default to flv
        let ext = suffix.unwrap_or("flv");

        // Build output path: {output_dir}/{YYYY-MM-DD}/{HH-MM-SS}_{sanitized_title}.{ext}
        let now = Local::now();
        let date_str = now.format("%Y-%m-%d").to_string();
        let time_str = now.format("%H-%M-%S").to_string();

        // Sanitize the title for use as a filename component
        let safe_title = sanitize_filename(stream_title);
        let safe_title = if safe_title.is_empty() { "live" } else { &safe_title };

        let output_path = format!(
            "{}/{}/{}_{}.{}",
            output_dir.trim_end_matches('/'),
            date_str,
            time_str,
            safe_title,
            ext
        );

        // Create output directory
        if let Some(parent) = std::path::Path::new(&output_path).parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                error!(
                    "Failed to create output directory {:?}: {}",
                    parent, e
                );
                return;
            }
        }

        info!(
            "🎬 Starting ffmpeg for room {} -> {}",
            room_id, output_path
        );

        // Build HTTP headers for ffmpeg's -headers argument.
        // ffmpeg expects headers as CRLF-separated "Key: Value" lines.
        let header_str = stream_headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\r\n");

        // Build ffmpeg command.
        // Use -headers for all HTTP headers (cookie, referer, etc.) so Bilibili
        // CDN knows the request is properly authenticated.
        let ffmpeg_cmd = format!(
            "{} -headers \"{}\" -i \"{}\" -c copy -f flv \"{}\"",
            self.ffmpeg_path, header_str, stream_url, output_path
        );

        debug!("ffmpeg command: {}", ffmpeg_cmd);

        // Spawn ffmpeg via shell so the quoting/escaping is handled properly
        let child = match Command::new("sh")
            .arg("-c")
            .arg(&ffmpeg_cmd)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                error!("Failed to spawn ffmpeg for room {}: {}", room_id, e);
                error!("Is '{}' installed and in PATH?", self.ffmpeg_path);
                return;
            }
        };

        // Store the handle
        self.processes.write().await.insert(
            room_id,
            RecorderHandle {
                child,
                output_path: output_path.clone(),
            },
        );

        info!("✅ Recording started for room {} at {}", room_id, output_path);
    }

    /// Stop recording a room (kill ffmpeg).
    /// Returns the path of the recorded file, or None if no recording was active.
    pub async fn stop(&self, room_id: u64) -> Option<String> {
        let handle = self.processes.write().await.remove(&room_id);

        match handle {
            Some(mut h) => {
                info!("🛑 Stopping recording for room {}: {}", room_id, h.output_path);
                // Send SIGTERM first, then kill on error
                if let Err(e) = h.child.kill().await {
                    error!("Failed to kill ffmpeg for room {}: {}", room_id, e);
                }
                // Wait for the process to fully exit (ignore result)
                let _ = h.child.wait().await;
                info!("✅ Recording stopped for room {}", room_id);
                Some(h.output_path)
            }
            None => {
                warn!("No active recording found for room {}", room_id);
                None
            }
        }
    }

    /// Kill all active recordings. Called on graceful shutdown.
    pub async fn stop_all(&self) {
        let mut procs = self.processes.write().await;
        let count = procs.len();

        if count == 0 {
            return;
        }

        info!("🛑 Stopping all {} active recording(s)...", count);

        for (room_id, mut handle) in procs.drain() {
            info!("  Killing ffmpeg for room {} ({})", room_id, handle.output_path);
            if let Err(e) = handle.child.kill().await {
                error!("  Failed to kill ffmpeg for room {}: {}", room_id, e);
            }
            let _ = handle.child.wait().await;
        }

        info!("✅ All recordings stopped.");
    }
}

/// Replace characters that are unsafe in filenames.
fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\x00'..='\x1F' => '_',
            _ => c,
        })
        .collect::<String>()
        .chars()
        .take(80) // keep filename reasonably short
        .collect()
}
