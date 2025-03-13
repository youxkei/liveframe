package main

import (
	"context"
	"encoding/json"
	"fmt"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"time"

	"golang.org/x/oauth2"
	"golang.org/x/oauth2/google"
)

// LoadClientSecretFromFile loads the client OAuth secret from the specified file
func LoadClientSecretFromFile(filePath string) (*oauth2.Config, error) {
	// Read the secret file
	b, err := os.ReadFile(filePath)
	if err != nil {
		return nil, fmt.Errorf("error reading client secret file: %v", err)
	}

	// Configure the OAuth client
	config, err := google.ConfigFromJSON(b, "https://www.googleapis.com/auth/youtube.readonly")
	if err != nil {
		return nil, fmt.Errorf("error parsing client secret file: %v", err)
	}

	// Set the redirect URL
	config.RedirectURL = "http://localhost:8080/oauth2callback"

	return config, nil
}

// TokenFile returns the path to the token storage file
func TokenFile() (string, error) {
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("failed to get user home directory: %w", err)
	}
	return filepath.Join(home, ".liveframe", "token.json"), nil
}

// TokenFromFile retrieves a token from a local file
func TokenFromFile(file string) (*oauth2.Token, error) {
	f, err := os.Open(file)
	if err != nil {
		return nil, fmt.Errorf("failed to open token file: %w", err)
	}
	defer f.Close()
	tok := &oauth2.Token{}
	err = json.NewDecoder(f).Decode(tok)
	if err != nil {
		return tok, fmt.Errorf("failed to decode token from file: %w", err)
	}
	return tok, nil
}

// SaveToken saves a token to a file
func SaveToken(path string, token *oauth2.Token) error {
	// Ensure directory exists
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0700); err != nil {
		return fmt.Errorf("failed to create token directory: %w", err)
	}

	f, err := os.OpenFile(path, os.O_RDWR|os.O_CREATE|os.O_TRUNC, 0600)
	if err != nil {
		return fmt.Errorf("failed to open token file for writing: %w", err)
	}
	defer f.Close()

	if err := json.NewEncoder(f).Encode(token); err != nil {
		return fmt.Errorf("failed to encode token to file: %w", err)
	}
	return nil
}

// GetOAuthClient creates an HTTP server for OAuth flow and returns authorized client
// If forceAuth is true, it will start a new OAuth flow regardless of existing token
func GetOAuthClient(ctx context.Context, config *oauth2.Config, forceAuth bool) (*http.Client, error) {
	// Try to load token from file
	tokenPath, err := TokenFile()
	if err != nil {
		return nil, fmt.Errorf("failed to get token file path: %w", err)
	}

	// If not forcing auth, try to use existing token
	if !forceAuth {
		tok, err := TokenFromFile(tokenPath)
		if err == nil {
			// Token exists and was loaded successfully
			return config.Client(ctx, tok), nil
		}
	}

	// If forceAuth is true or token doesn't exist/is invalid - start OAuth flow
	log.Println("Starting new OAuth authentication flow")
	var tok *oauth2.Token
	codeChan := make(chan string)
	var wg sync.WaitGroup
	wg.Add(1)

	// Create a server mux for the HTTP server
	mux := http.NewServeMux()

	// Create an HTTP server to handle the OAuth callback
	server := &http.Server{
		Addr:    ":8080",
		Handler: mux,
	}

	// Set up the handler for the OAuth callback
	mux.HandleFunc("/oauth2callback", func(w http.ResponseWriter, r *http.Request) {
		code := r.URL.Query().Get("code")
		codeChan <- code

		// Display success message
		w.Header().Set("Content-Type", "text/html")
		fmt.Fprintf(w, "<h1>Authorization Successful</h1><p>You can close this window now.</p>")

		// Shutdown the server after a short delay
		go func() {
			time.Sleep(2 * time.Second)
			server.Shutdown(ctx)
			wg.Done()
		}()
	})

	// Start the HTTP server
	go func() {
		if err := server.ListenAndServe(); err != http.ErrServerClosed {
			log.Printf("HTTP server error: %v", err)
		}
	}()

	// Generate the authorization URL with maximum token lifetime
	// AccessTypeOffline provides a refresh token
	// ApprovalForce ensures we get a fresh refresh token by forcing the consent screen
	authURL := config.AuthCodeURL(
		"state-token",
		oauth2.AccessTypeOffline,
		oauth2.ApprovalForce,
	)

	// Open the URL in browser
	log.Printf("Opening browser for OAuth authorization: %s", authURL)
	if err := OpenURL(authURL); err != nil {
		log.Printf("Failed to open browser automatically: %v", err)
		fmt.Printf("Please open the following URL in your browser to authorize the application:\n%v\n", authURL)
	} else {
		log.Println("Browser opened for authorization")
	}

	// Wait for the authorization code
	code := <-codeChan

	// Exchange the code for a token
	tok, err = config.Exchange(ctx, code)
	if err != nil {
		return nil, fmt.Errorf("error exchanging code for token: %v", err)
	}

	// Save the token for future use
	if err := SaveToken(tokenPath, tok); err != nil {
		log.Printf("Warning: Failed to save token: %v", err)
	}

	// Wait for the server to shutdown
	wg.Wait()

	return config.Client(ctx, tok), nil
}
