mod bilibili;
mod config;
mod monitor;
mod recorder;
mod server;
mod uploader;

use config::{AppConfig, CookiesFile};
use monitor::{run_monitor, AppState};
use tracing::{error, info};

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_ids(false)
        .with_line_number(false)
        .init();

    // Load config
    let config = match AppConfig::load("config.json") {
        Ok(c) => {
            info!(
                "Loaded config: {} stream(s), interval={}s, ffmpeg={}",
                c.streamers.len(),
                c.interval,
                c.ffmpeg_path
            );
            if c.upload.is_some() {
                info!("Upload is configured and enabled");
            }
            c
        }
        Err(e) => {
            error!("Failed to load config.json: {}", e);
            std::process::exit(1);
        }
    };

    // Load cookies
    let cookies = match CookiesFile::load("cookies.json") {
        Ok(c) => {
            info!("Loaded cookies ({} entries)", c.cookie_info.cookies.len());
            c
        }
        Err(e) => {
            error!("Failed to load cookies.json: {}", e);
            std::process::exit(1);
        }
    };

    // Extract upload config before moving config into monitor
    let upload_config = config.upload.clone();
    let cookies_path = "cookies.json".to_string();

    // Shared application state
    let state = AppState::new(&config.ffmpeg_path, cookies_path, upload_config);

    // Spawn the monitor in the background
    let monitor_state = state.clone();
    let monitor_config = config.clone();
    let monitor_handle = tokio::spawn(async move {
        run_monitor(monitor_config, cookies, monitor_state).await;
    });

    // Start the web server (use config values)
    let bind_addr = format!("{}:{}", config.server.addr, config.server.port);
    let app = server::build_router(state.clone());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
    info!("Web server listening on http://{}", bind_addr);

    // Graceful shutdown on Ctrl+C
    let server = axum::serve(listener, app);

    tokio::select! {
        result = server => {
            if let Err(e) = result {
                error!("Server error: {}", e);
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("\nShutdown signal received (Ctrl+C)...");
        }
    }

    // Stop all active recordings
    state.recorder.stop_all().await;

    // Abort the monitor task
    monitor_handle.abort();
    let _ = monitor_handle.await;

    info!("Goodbye.");
}
