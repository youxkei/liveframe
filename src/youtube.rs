use log::{debug, error, info};
use reqwest;
use serde_json;

use crate::models::LiveBroadcastsResponse;

// Returns Some(video_id) when a live broadcast is active, None otherwise.
// The YouTube broadcast ID is identical to the video ID.
pub async fn check_youtube_streaming(access_token: &str) -> std::result::Result<Option<String>, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    debug!("Calling YouTube API to check streaming status...");
    let response = client
        .get("https://www.googleapis.com/youtube/v3/liveBroadcasts")
        .query(&[
            ("part", "id,snippet,status"),
            ("broadcastStatus", "active"),
        ])
        .header("Authorization", format!("Bearer {}", access_token))
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await?;
        error!("YouTube API returned error: {}", error_text);
        return Err(format!("YouTube API error: {}", error_text).into());
    }

    let response_text = response.text().await?;
    let broadcasts: LiveBroadcastsResponse = serde_json::from_str(&response_text)?;

    info!("Found {} broadcasts", broadcasts.items.len());

    for (i, broadcast) in broadcasts.items.iter().enumerate() {
        info!("Broadcast #{}: ID={}, Title={}, Status={:?}",
            i + 1,
            broadcast.id,
            broadcast.snippet.title,
            broadcast.status.life_cycle_status
        );
    }

    let video_id = broadcasts.items.into_iter()
        .find(|b| b.status.life_cycle_status.as_deref() == Some("live"))
        .map(|b| b.id);

    Ok(video_id)
}