use crate::config::{StreamConfig, UploadConfig};
use biliup::credential;
use biliup::uploader::bilibili::{Studio, Video};
use biliup::uploader::line;
use biliup::uploader::VideoFile;
use std::path::Path;
use tracing::{error, info, warn};

/// Upload a recorded video file to Bilibili.
///
/// `cookies_path` — path to the cookies.json file (must match `LoginInfo` format).
/// `stream` — the streamer config entry for this recording.
/// `upload_cfg` — the global upload config from config.json.
/// `file_path` — absolute path to the recorded video file.
/// `stream_title` — the live stream title (used in title template expansion).
pub async fn upload_video(
    cookies_path: &str,
    stream: &StreamConfig,
    upload_cfg: &UploadConfig,
    file_path: &str,
    stream_title: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    info!(
        "Starting upload for {} (room {}): {}",
        stream.name, stream.room_id, file_path
    );

    // 1. Build the BiliBili client from the cookies file
    let bili = credential::bilibili_from_cookies(cookies_path, None)
        .map_err(|e| format!("Failed to create BiliBili client: {e}"))?;

    // 2. Open the recorded video file
    let vid_path = Path::new(file_path);
    let video_file = VideoFile::new(vid_path)
        .map_err(|e| format!("Failed to open video file '{file_path}': {e}"))?;

    info!(
        "Video file: {} ({} bytes)",
        video_file.file_name, video_file.total_size
    );

    // 3. Select upload CDN line (use Bilibili self-built DSA by default)
    let upload_line = line::bldsa();
    info!("Using Bilibili self-built DSA upload line");

    // 4. Pre-upload — reserves an upload session on the CDN
    let parcel = upload_line
        .pre_upload(&bili, video_file)
        .await
        .map_err(|e| format!("Pre-upload failed: {e}"))?;

    // 5. Upload the video data chunks
    let stateless_client = biliup::client::StatelessClient::default();
    let video: Video = parcel
        .upload(stateless_client, 3, |vs| {
            // Convert io::Result<Bytes> → Result<(Bytes, usize), biliup::error::Kind>
            futures::StreamExt::map(vs, |r| {
                r.map(|b| {
                    let len = b.len();
                    (b, len)
                })
                .map_err(biliup::error::Kind::from)
            })
        })
        .await
        .map_err(|e| format!("Upload failed: {e}"))?;

    info!("Upload complete, server filename: {}", video.filename);

    // 6. Build submission metadata with template expansion
    let title = expand_title(&upload_cfg.title, &stream.name, stream_title);
    let source = expand_simple(&upload_cfg.source, &stream.name, stream_title, stream.room_id);
    let tags = upload_cfg.tags.join(",");
    let desc = if upload_cfg.desc.is_empty() {
        String::new()
    } else {
        expand_title(&upload_cfg.desc, &stream.name, stream_title)
    };
    let dynamic = if upload_cfg.dynamic.is_empty() {
        String::new()
    } else {
        expand_title(&upload_cfg.dynamic, &stream.name, stream_title)
    };

    let studio = Studio {
        copyright: upload_cfg.copyright,
        source,
        tid: upload_cfg.tid,
        cover: String::new(),
        title: title.clone(),
        desc_format_id: 0,
        desc,
        desc_v2: None,
        dynamic,
        subtitle: Default::default(),
        tag: tags,
        videos: vec![video],
        dtime: None,
        open_subtitle: false,
        interactive: 0,
        mission_id: None,
        dolby: 0,
        lossless_music: 0,
        no_reprint: 0,
        is_only_self: None,
        charging_pay: 0,
        aid: None,
        up_selection_reply: false,
        up_close_reply: false,
        up_close_danmu: false,
        extra_fields: None,
    };

    info!("Submitting video with title: \"{}\"", title);

    // 7. Submit via App API, fall back to Web on failure
    match bili.submit_by_app(&studio, None).await {
        Ok(response) => {
            info!(
                "Upload submission successful (app)! code={}",
                response.code
            );
            Ok(())
        }
        Err(e) => {
            warn!(
                "App submission failed ({}), trying web submission...",
                e
            );
            match bili.submit_by_web(&studio, None).await {
                Ok(response) => {
                    info!(
                        "Upload submission successful (web)! code={}",
                        response.code
                    );
                    Ok(())
                }
                Err(e2) => {
                    error!(
                        "Both app and web submission failed: {} / {}",
                        e, e2
                    );
                    Err(format!("Both app and web submission failed: {e} / {e2}").into())
                }
            }
        }
    }
}

/// Expand a title template: replace `{streamer.name}`, `{stream.title}`, and
/// apply `chrono` format specifiers (e.g. `%Y-%m-%d %H_%M_%S`).
fn expand_title(template: &str, streamer_name: &str, stream_title: &str) -> String {
    let now = chrono::Local::now();
    let s = template
        .replace("{streamer.name}", streamer_name)
        .replace("{stream.title}", stream_title);
    now.format(&s).to_string()
}

/// Expand simple template variables without chrono formatting.
fn expand_simple(
    template: &str,
    streamer_name: &str,
    stream_title: &str,
    room_id: u64,
) -> String {
    template
        .replace("{streamer.name}", streamer_name)
        .replace("{stream.title}", stream_title)
        .replace("{streamer.room_id}", &room_id.to_string())
}
