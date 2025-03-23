use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::Utc;
use dirs::home_dir;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server, StatusCode};
use log::{debug, error, info, warn};
use tokio::sync::oneshot;
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use oauth2::basic::BasicClient;
use windows::core::*;
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOW;

use crate::models::{ClientSecrets, OAuthState, TokenInfo};

// Maximum number of retries for network operations
const MAX_RETRIES: u32 = 3;
// Delay between retries in seconds
const RETRY_DELAY: u64 = 5;

// Function to open a URL in the default browser
pub fn open_url_in_browser(url: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    info!("Opening URL in browser: {}", url);
    
    // Convert the URL to a wide string for Windows API
    let url_wide: Vec<u16> = url.encode_utf16().chain(std::iter::once(0)).collect();
    
    unsafe {
        // Use ShellExecuteW to open the URL in the default browser
        let result = ShellExecuteW(
            None,
            w!("open"),
            PCWSTR::from_raw(url_wide.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOW,
        );
        
        // Check if the operation was successful
        if result.0 <= 32 {
            error!("Failed to open URL in browser, error code: {}", result.0);
            return Err(format!("Failed to open URL in browser, error code: {}", result.0).into());
        }
    }
    
    Ok(())
}

// Function to get OAuth token (either from file or through auth flow)
pub async fn get_oauth_token() -> std::result::Result<TokenInfo, Box<dyn std::error::Error>> {
    // Check if token file exists
    let token_path = get_token_path()?;
    if token_path.exists() {
        info!("Found existing token file, loading...");
        let mut file = File::open(&token_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        
        let token_info: TokenInfo = serde_json::from_str(&contents)?;
        
        // If token is not expired, return it
        if Utc::now() < token_info.expiry {
            debug!("Token is still valid, using existing token");
            return Ok(token_info);
        }
        
        // If token is expired, try to refresh it with retry logic
        info!("Token expired, refreshing...");
        let mut retry_count = 0;
        loop {
            match refresh_token(&token_info.refresh_token).await {
                Ok(new_token) => return Ok(new_token),
                Err(e) => {
                    // Check if this is a network error that we can retry
                    if retry_count >= MAX_RETRIES {
                        warn!("Failed to refresh token after {} retries: {}, starting new auth flow", MAX_RETRIES, e);
                        break;
                    }
                    
                    retry_count += 1;
                    warn!("Network error while refreshing token (attempt {}/{}): {}",
                          retry_count, MAX_RETRIES, e);
                    info!("Retrying in {} seconds...", RETRY_DELAY);
                    tokio::time::sleep(Duration::from_secs(RETRY_DELAY)).await;
                }
            }
        }
    }
    
    // If no valid token exists or refresh failed, start OAuth flow with retry logic
    info!("Starting OAuth authentication flow...");
    let mut retry_count = 0;
    let token_info = loop {
        match oauth_flow().await {
            Ok(token) => break token,
            Err(e) => {
                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    return Err(format!("Failed to complete OAuth flow after {} retries: {}", MAX_RETRIES, e).into());
                }
                
                warn!("Error during OAuth flow (attempt {}/{}): {}",
                      retry_count, MAX_RETRIES, e);
                info!("Retrying in {} seconds...", RETRY_DELAY);
                tokio::time::sleep(Duration::from_secs(RETRY_DELAY)).await;
            }
        }
    };
    
    // Save token to file
    save_token(&token_info)?;
    debug!("Token saved to file");
    
    Ok(token_info)
}

// Function to get the path to the token file
pub fn get_token_path() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = home_dir().ok_or("Could not find home directory")?;
    path.push(".liveframe");
    
    // Create directory if it doesn't exist
    if !path.exists() {
        fs::create_dir_all(&path)?;
    }
    
    path.push("token.json");
    Ok(path)
}

// Function to get the path to the client secrets file
pub fn get_secrets_path() -> std::result::Result<PathBuf, Box<dyn std::error::Error>> {
    let mut path = home_dir().ok_or("Could not find home directory")?;
    path.push(".liveframe");
    path.push("secret.json");
    
    if !path.exists() {
        return Err("Client secrets file not found at ~/.liveframe/secret.json".into());
    }
    
    Ok(path)
}

// Function to save token to file
pub fn save_token(token_info: &TokenInfo) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let token_path = get_token_path()?;
    let json = serde_json::to_string_pretty(token_info)?;
    let mut file = File::create(token_path)?;
    file.write_all(json.as_bytes())?;
    Ok(())
}

// Function to load client secrets
pub fn load_client_secrets() -> std::result::Result<ClientSecrets, Box<dyn std::error::Error>> {
    let secrets_path = get_secrets_path()?;
    let mut file = File::open(secrets_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let secrets: ClientSecrets = serde_json::from_str(&contents)?;
    Ok(secrets)
}

// Function to perform OAuth flow
pub async fn oauth_flow() -> std::result::Result<TokenInfo, Box<dyn std::error::Error>> {
    // Load client secrets
    info!("Loading client secrets...");
    let secrets = load_client_secrets()?;
    
    // Create OAuth client
    debug!("Creating OAuth client...");
    let client = BasicClient::new(
        ClientId::new(secrets.installed.client_id),
        Some(ClientSecret::new(secrets.installed.client_secret)),
        AuthUrl::new(secrets.installed.auth_uri)?,
        Some(TokenUrl::new(secrets.installed.token_uri)?),
    )
    .set_redirect_uri(RedirectUrl::new("http://localhost:8080".to_string())?);
    
    // Generate PKCE challenge
    debug!("Generating PKCE challenge...");
    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    
    // Generate the authorization URL
    let (auth_url, csrf_state) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("https://www.googleapis.com/auth/youtube.readonly".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();
    
    info!("Opening authorization URL in browser...");
    
    // Open the URL in the default browser
    if let Err(e) = open_url_in_browser(auth_url.as_str()) {
        warn!("Failed to open URL in browser: {}", e);
        // Fallback to displaying the URL if we can't open the browser
        info!("Please open this URL in your browser to authorize the application:");
        info!("{}", auth_url);
    }
    
    // Create a channel to signal when the authorization code is received
    let (tx, rx) = oneshot::channel::<()>();
    
    // Create a shared state for the callback server
    let state = Arc::new(Mutex::new(OAuthState {
        auth_code: None,
        csrf_state: csrf_state.secret().clone(),
        pkce_verifier: Some(pkce_verifier),
        auth_code_received_tx: Some(tx),
    }));
    
    // Start the HTTP server for the OAuth callback
    info!("Starting OAuth callback server on http://localhost:8080");
    let state_clone = state.clone();
    let make_service = make_service_fn(move |_| {
        let state = state_clone.clone();
        async move {
            Ok::<_, hyper::Error>(service_fn(move |req| {
                let state = state.clone();
                async move { handle_oauth_callback(req, state).await }
            }))
        }
    });
    
    let addr = ([127, 0, 0, 1], 8080).into();
    let server = Server::bind(&addr).serve(make_service);
    
    // Run the server with a timeout
    debug!("Waiting for authorization callback (timeout: 2 minutes)...");
    let server_with_timeout = async move {
        let server_future = server.with_graceful_shutdown(async {
            // Wait for the auth code to be received or timeout after 2 minutes
            tokio::select! {
                _ = rx => {
                    debug!("Authorization code received, shutting down server");
                }
                _ = tokio::time::sleep(Duration::from_secs(120)) => {
                    warn!("Timeout waiting for authorization (2 minutes elapsed)");
                }
            }
        });
        
        server_future.await
    };
    
    // Run the server and wait for it to complete
    server_with_timeout.await?;
    
    // Get the authorization code from the state
    let auth_code = {
        let state_guard = state.lock().unwrap();
        state_guard.auth_code.clone().ok_or("No authorization code received")?
    };
    
    // Get the PKCE verifier from the state
    let pkce_verifier = {
        let mut state_guard = state.lock().unwrap();
        state_guard.pkce_verifier.take().ok_or("PKCE verifier not found")?
    };
    
    // Exchange the authorization code for an access token
    info!("Exchanging authorization code for access token...");
    let token_result = client
        .exchange_code(AuthorizationCode::new(auth_code))
        .set_pkce_verifier(pkce_verifier)
        .request_async(oauth2::reqwest::async_http_client)
        .await?;
    
    // Create token info
    debug!("Creating token info with expiry time");
    let token_info = TokenInfo {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token()
            .ok_or("No refresh token received")?
            .secret()
            .clone(),
        expiry: Utc::now() + chrono::Duration::seconds(token_result.expires_in().unwrap_or_default().as_secs() as i64),
    };
    
    info!("OAuth flow completed successfully");
    Ok(token_info)
}

// Function to handle OAuth callback
pub async fn handle_oauth_callback(
    req: Request<Body>,
    state: Arc<Mutex<OAuthState>>,
) -> std::result::Result<Response<Body>, hyper::Error> {
    let uri = req.uri();
    let query = uri.query().unwrap_or("");
    
    let params: HashMap<_, _> = url::form_urlencoded::parse(query.as_bytes())
        .into_owned()
        .collect();
    
    let mut response = Response::new(Body::empty());
    
    if let (Some(code), Some(received_state)) = (params.get("code"), params.get("state")) {
        // Verify CSRF state
        let expected_state = {
            let state_guard = state.lock().unwrap();
            state_guard.csrf_state.clone()
        };
        
        if received_state == &expected_state {
            // Store the authorization code and signal that it's been received
            {
                let mut state_guard = state.lock().unwrap();
                state_guard.auth_code = Some(code.clone());
                
                // Send signal through the channel if it exists
                if let Some(tx) = state_guard.auth_code_received_tx.take() {
                    let _ = tx.send(());
                    debug!("Sent signal that authorization code was received");
                }
            }
            
            *response.body_mut() = Body::from(
                "Authorization successful! You can close this window and return to the application.",
            );
        } else {
            *response.status_mut() = StatusCode::BAD_REQUEST;
            *response.body_mut() = Body::from("Invalid state parameter");
        }
    } else {
        *response.status_mut() = StatusCode::BAD_REQUEST;
        *response.body_mut() = Body::from("Missing code or state parameter");
    }
    
    Ok(response)
}

// Function to refresh OAuth token
pub async fn refresh_token(refresh_token: &str) -> std::result::Result<TokenInfo, Box<dyn std::error::Error>> {
    // Load client secrets
    debug!("Loading client secrets for token refresh...");
    let secrets = load_client_secrets()?;
    
    // Create OAuth client
    let client = BasicClient::new(
        ClientId::new(secrets.installed.client_id),
        Some(ClientSecret::new(secrets.installed.client_secret)),
        AuthUrl::new(secrets.installed.auth_uri)?,
        Some(TokenUrl::new(secrets.installed.token_uri)?),
    );
    
    // Exchange the refresh token for a new access token with retry logic
    info!("Exchanging refresh token for new access token...");
    
    let mut retry_count = 0;
    let token_result = loop {
        match client
            .exchange_refresh_token(&RefreshToken::new(refresh_token.to_string()))
            .request_async(oauth2::reqwest::async_http_client)
            .await
        {
            Ok(token) => break token,
            Err(e) => {
                retry_count += 1;
                if retry_count >= MAX_RETRIES {
                    return Err(format!("Failed to refresh token after {} retries: {}", MAX_RETRIES, e).into());
                }
                
                warn!("Network error while refreshing token (attempt {}/{}): {}",
                      retry_count, MAX_RETRIES, e);
                info!("Retrying in {} seconds...", RETRY_DELAY);
                tokio::time::sleep(Duration::from_secs(RETRY_DELAY)).await;
            }
        }
    };
    
    // Create token info
    let token_info = TokenInfo {
        access_token: token_result.access_token().secret().clone(),
        refresh_token: token_result.refresh_token()
            .map(|rt| rt.secret().clone())
            .unwrap_or_else(|| refresh_token.to_string()),
        expiry: Utc::now() + chrono::Duration::seconds(token_result.expires_in().unwrap_or_default().as_secs() as i64),
    };
    
    // Save the new token
    debug!("Saving refreshed token to file...");
    save_token(&token_info)?;
    info!("Token refreshed successfully");
    
    Ok(token_info)
}