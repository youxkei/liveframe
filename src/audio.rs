use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Context};
use ebur128::{EbuR128, Mode};
use log::{error, info, warn};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use windows::Win32::Foundation::HWND;
use yt_dlp::Downloader;
use yt_dlp::client::deps::Libraries;

use crate::window::{set_color_state, COLOR_GREEN, COLOR_RED};

const SAMPLE_RATE: u32 = 48_000;
const CHANNELS: u32 = 2;
// f32 little-endian, interleaved stereo
const BYTES_PER_FRAME: usize = (CHANNELS as usize) * 4;

// Momentary loudness below this is considered silent.
const SILENCE_THRESHOLD_LUFS: f64 = -50.0;
// How often we sample momentary loudness and make a color decision.
const EVAL_INTERVAL: Duration = Duration::from_millis(100);
// Green -> Red requires this many consecutive silent evaluations (~500ms).
const SILENCE_EVALS_TO_RED: u32 = 5;

// HWND is not Send; wrap it so we can move it into the spawned audio task.
#[derive(Clone, Copy)]
pub struct SendHwnd(pub HWND);
unsafe impl Send for SendHwnd {}
unsafe impl Sync for SendHwnd {}

fn libs_dir() -> anyhow::Result<PathBuf> {
    let home = dirs::home_dir().ok_or_else(|| anyhow!("no home dir"))?;
    Ok(home.join(".liveframe").join("libs"))
}

// Resolve HLS manifest URL for a YouTube live video and stream audio through ffmpeg,
// measuring momentary loudness and updating the window frame color.
pub async fn run_audio_task(
    video_id: String,
    hwnd: SendHwnd,
    cancel: CancellationToken,
) -> anyhow::Result<()> {
    let libs = libs_dir()?;
    tokio::fs::create_dir_all(&libs).await.ok();

    // Libraries::new() expects FULL PATHS to the binary files. If a directory with the
    // same name exists, install_dependencies skips the download silently and leaves the
    // directory path as the "binary path" — which later fails with "access denied" when
    // fetch_video_infos tries to spawn a directory as a process.
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let yt_dlp_path = libs.join(format!("yt-dlp{}", exe));
    let ffmpeg_path_expected = libs.join(format!("ffmpeg{}", exe));

    info!("Installing yt-dlp/ffmpeg binaries into {:?} (first run only)", libs);
    let libraries = Libraries::new(yt_dlp_path, ffmpeg_path_expected)
        .install_dependencies()
        .await
        .context("failed to install yt-dlp/ffmpeg")?;
    let ffmpeg_path = libraries.ffmpeg.clone();
    info!("yt-dlp: {:?} / ffmpeg: {:?}", libraries.youtube, ffmpeg_path);

    let downloader = Downloader::builder(libraries, libs.join("output"))
        .build()
        .await
        .context("failed to build yt-dlp Downloader")?;

    let watch_url = format!("https://www.youtube.com/watch?v={}", video_id);
    info!("Fetching video info for {}", watch_url);
    let video = downloader
        .fetch_video_infos(watch_url)
        .await
        .context("failed to fetch video info")?;

    if !video.is_currently_live() {
        return Err(anyhow!("video {} is not currently live", video_id));
    }

    let hls_url = pick_hls_url(&video)?;
    info!("Using HLS URL (truncated): {}...", &hls_url[..hls_url.len().min(80)]);

    info!("Launching ffmpeg: {:?}", ffmpeg_path);

    let mut child = Command::new(&ffmpeg_path)
        .args([
            "-loglevel", "error",
            "-nostdin",
            "-i", &hls_url,
            "-vn",
            "-f", "f32le",
            "-acodec", "pcm_f32le",
            "-ac", &CHANNELS.to_string(),
            "-ar", &SAMPLE_RATE.to_string(),
            "-",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("failed to spawn ffmpeg")?;

    let mut stdout = child.stdout.take().ok_or_else(|| anyhow!("ffmpeg stdout missing"))?;
    let stderr = child.stderr.take();
    if let Some(mut stderr) = stderr {
        tokio::spawn(async move {
            let mut buf = String::new();
            let _ = tokio::io::AsyncReadExt::read_to_string(&mut stderr, &mut buf).await;
            if !buf.is_empty() {
                warn!("ffmpeg stderr: {}", buf.trim_end());
            }
        });
    }

    let mut ebu = EbuR128::new(CHANNELS, SAMPLE_RATE, Mode::M).context("EbuR128 init")?;

    // Read ~20ms of audio per iteration: 48000 * 0.02 * 2ch * 4B = 7680 bytes
    const CHUNK_BYTES: usize = 7680;
    let mut byte_buf = vec![0u8; CHUNK_BYTES];
    let mut sample_buf: Vec<f32> = Vec::with_capacity(CHUNK_BYTES / 4);

    let mut last_eval = std::time::Instant::now();
    let mut silent_evals: u32 = 0;

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("audio task cancelled for video {}", video_id);
                let _ = child.kill().await;
                return Ok(());
            }
            res = stdout.read(&mut byte_buf) => {
                let n = match res {
                    Ok(0) => {
                        info!("ffmpeg stdout EOF for video {}", video_id);
                        let _ = child.wait().await;
                        return Ok(());
                    }
                    Ok(n) => n,
                    Err(e) => {
                        error!("ffmpeg read error: {}", e);
                        let _ = child.kill().await;
                        return Err(e.into());
                    }
                };

                // Only process a frame-aligned portion. Any trailing partial frame is rare
                // because ffmpeg writes complete frames, but guard anyway.
                let aligned = n - (n % BYTES_PER_FRAME);
                sample_buf.clear();
                for chunk in byte_buf[..aligned].chunks_exact(4) {
                    sample_buf.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
                }
                if let Err(e) = ebu.add_frames_f32(&sample_buf) {
                    error!("ebur128 add_frames_f32 error: {}", e);
                    continue;
                }

                if last_eval.elapsed() >= EVAL_INTERVAL {
                    last_eval = std::time::Instant::now();
                    match ebu.loudness_momentary() {
                        Ok(lufs) => {
                            let audible = lufs > SILENCE_THRESHOLD_LUFS && lufs.is_finite();
                            if audible {
                                silent_evals = 0;
                                set_color_state(hwnd.0, COLOR_GREEN);
                            } else {
                                silent_evals = silent_evals.saturating_add(1);
                                if silent_evals >= SILENCE_EVALS_TO_RED {
                                    set_color_state(hwnd.0, COLOR_RED);
                                }
                            }
                        }
                        Err(e) => {
                            // loudness_momentary needs >=400ms of samples; early reads return error.
                            log::debug!("ebur128 loudness_momentary error: {}", e);
                        }
                    }
                }
            }
        }
    }
}

fn pick_hls_url(video: &yt_dlp::model::Video) -> anyhow::Result<String> {
    let live_formats = video.live_formats();
    if live_formats.is_empty() {
        return Err(anyhow!("no HLS (M3U8Native) formats available"));
    }

    // Prefer an audio-only format (lowest bandwidth for our audio-only needs).
    let chosen = live_formats
        .iter()
        .find(|f| f.is_audio() && !f.is_video())
        .copied()
        .or_else(|| live_formats.first().copied())
        .ok_or_else(|| anyhow!("no suitable live format"))?;

    Ok(chosen.url().map_err(|e| anyhow!("format has no url: {}", e))?.clone())
}

