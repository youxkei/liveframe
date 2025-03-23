mod models;
mod oauth;
mod window;
mod youtube;

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::Utc;
use env_logger::Builder;
use log::{debug, error, info};

#[tokio::main]
async fn main() -> windows::core::Result<()> {
    // Initialize the logger with timestamps
    Builder::new()
        .format(|buf, record| {
            use std::io::Write;
            writeln!(
                buf,
                "{} [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.args()
            )
        })
        .filter(None, log::LevelFilter::Info)
        .init();

    info!("Application starting...");

    // Create a channel for sending the window handle from the window thread to the main thread
    let (tx, rx) = mpsc::channel();

    // Spawn a thread to create the window and run the message loop
    let _window_thread = thread::spawn(move || unsafe { window::create_window_and_run_message_loop(tx) });

    // Wait to receive the window handle from the window thread
    let hwnd = match rx.recv() {
        Ok(handle) => handle,
        Err(e) => {
            error!("Failed to receive window handle: {}", e);
            return Err(windows::core::Error::from_win32());
        }
    };

    // Initially hide the window until we check streaming status
    unsafe {
        if hwnd.0 != 0 {
            window::set_window_visibility(hwnd, false);
            debug!("Window initially hidden");
        }
    }

    // Setup Ctrl+C handler for graceful exit
    ctrlc::set_handler(move || {
        info!("Received Ctrl+C, exiting normally...");
        std::process::exit(0);
    })
    .expect("Error setting Ctrl+C handler");

    // Get OAuth token (either from file or through auth flow)
    let token_info = match oauth::get_oauth_token().await {
        Ok(token) => token,
        Err(e) => {
            error!("Failed to get OAuth token: {}", e);
            return Err(windows::core::Error::from_win32());
        }
    };

    // Main loop to check YouTube streaming status
    let mut is_streaming = false;
    let mut token = token_info;

    loop {
        // Check if token needs refresh
        let current_time = Utc::now();
        if current_time >= token.expiry {
            info!("Token expired, refreshing...");
            match oauth::refresh_token(&token.refresh_token).await {
                Ok(new_token) => token = new_token,
                Err(e) => error!("Failed to refresh token: {}", e),
            }
        }

        // Check YouTube streaming status
        debug!("Check streaming status...");
        match youtube::check_youtube_streaming(&token.access_token).await {
            Ok(streaming) => {
                debug!("Current streaming status: {}", streaming);
                
                if streaming != is_streaming {
                    is_streaming = streaming;
                    info!("Streaming status changed to: {}", is_streaming);
                    
                    // Update window visibility based on streaming status
                    unsafe {
                        window::set_window_visibility(hwnd, is_streaming);
                    }
                }
            }
            Err(e) => error!("Failed to check streaming status: {}", e),
        }

        // Sleep for 5 seconds before checking again
        thread::sleep(Duration::from_secs(5));
    }
}
