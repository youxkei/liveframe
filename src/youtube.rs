use log::{debug, error, info};
use reqwest;
use serde_json;

use crate::models::LiveBroadcastsResponse;

// Function to check if the user is currently streaming on YouTube
pub async fn check_youtube_streaming(access_token: &str) -> std::result::Result<bool, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    
    // Call the YouTube API to list live broadcasts
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
    
    // Parse the response
    let response_text = response.text().await?;
    let broadcasts: LiveBroadcastsResponse = serde_json::from_str(&response_text)?;
    
    // Log the number of broadcasts found
    info!("Found {} broadcasts", broadcasts.items.len());
    
    // Log details of each broadcast
    for (i, broadcast) in broadcasts.items.iter().enumerate() {
        info!("Broadcast #{}: ID={}, Title={}, Status={:?}",
            i + 1,
            broadcast.id,
            broadcast.snippet.title,
            broadcast.status.life_cycle_status
        );
    }
    
    // Check if there are any active broadcasts
    let is_streaming = broadcasts.items.iter().any(|broadcast| {
        broadcast.status.life_cycle_status.as_deref() == Some("live")
    });
    
    Ok(is_streaming)
}