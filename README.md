# LiveFrame-RS

A Windows application that displays a frame around your screen based on your YouTube streaming state.

## Features

- Shows a white frame when the app is running and no YouTube stream is active
- Shows a red or green frame around your screen when you're streaming on YouTube
- Uses YouTube API to detect active live broadcasts
- OAuth authentication for secure API access
- Automatically refreshes authentication tokens

## Setup

### 1. Create YouTube API Credentials

1. Go to the [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project or select an existing one
3. Enable the YouTube Data API v3 for your project
4. Create OAuth 2.0 credentials (OAuth client ID)
   - Application type: Desktop app
   - Name: LiveFrame (or any name you prefer)
5. Download the credentials JSON file

### 2. Set Up Authentication

1. Create a directory `~/.liveframe` in your home directory
2. Copy the downloaded credentials to `~/.liveframe/secret.json`

The secret.json file should have this structure:
```json
{
  "installed": {
    "client_id": "YOUR_CLIENT_ID",
    "client_secret": "YOUR_CLIENT_SECRET",
    "redirect_uris": ["http://localhost:8080"],
    "auth_uri": "https://accounts.google.com/o/oauth2/auth",
    "token_uri": "https://oauth2.googleapis.com/token"
  }
}
```

### 3. Run the Application

```
cargo run
```

On first run, the application will:
1. Open a browser window for YouTube authentication
2. Ask you to authorize the application
3. Save the authentication token to `~/.liveframe/token.json`
4. Start monitoring your YouTube streaming status

## Usage

- When the app is running and no stream is active, a white frame appears around your screen
- When you start streaming on YouTube, the frame changes to red or green based on stream audio
- The application checks your streaming status every 30 seconds

## Troubleshooting

- If authentication fails, delete the `~/.liveframe/token.json` file and restart the application
- Make sure your YouTube account has streaming permissions
- Check that the YouTube Data API v3 is enabled in your Google Cloud project
