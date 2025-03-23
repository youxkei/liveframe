use chrono::{DateTime, Utc};
use oauth2::PkceCodeVerifier;
use serde::{Deserialize, Serialize};

// Struct for OAuth client secrets
#[derive(Deserialize)]
pub struct ClientSecrets {
    pub installed: InstalledSecrets,
}

#[derive(Deserialize)]
pub struct InstalledSecrets {
    pub client_id: String,
    pub client_secret: String,
    pub auth_uri: String,
    pub token_uri: String,
}

// Struct for OAuth tokens
#[derive(Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub refresh_token: String,
    pub expiry: DateTime<Utc>,
}

// Struct for YouTube API response
#[derive(Deserialize)]
pub struct LiveBroadcastsResponse {
    pub items: Vec<LiveBroadcast>,
}

#[derive(Deserialize)]
pub struct LiveBroadcast {
    pub id: String,
    pub snippet: LiveBroadcastSnippet,
    pub status: LiveBroadcastStatus,
}

#[derive(Deserialize)]
pub struct LiveBroadcastSnippet {
    pub title: String,
}

#[derive(Deserialize, Debug)]
pub struct LiveBroadcastStatus {
    #[serde(default)]
    #[serde(rename = "lifeCycleStatus")]
    pub life_cycle_status: Option<String>,
}

// Global state for the OAuth callback server
pub struct OAuthState {
    pub auth_code: Option<String>,
    pub csrf_state: String,
    pub pkce_verifier: Option<PkceCodeVerifier>,
}