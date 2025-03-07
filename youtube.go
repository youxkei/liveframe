package main

import (
	"context"
	"log"
	"net/http"
	"time"

	"google.golang.org/api/option"
	"google.golang.org/api/youtube/v3"
)

// IsLiveStreaming checks if the user is currently live streaming on YouTube
func IsLiveStreaming(ctx context.Context, client *http.Client, checkInterval time.Duration) chan bool {
	// Use a buffered channel to prevent blocking
	statusCh := make(chan bool, 1)

	// Create YouTube service
	youtubeService, err := youtube.NewService(ctx, option.WithHTTPClient(client))
	if err != nil {
		log.Fatalf("Error creating YouTube service: %v", err)
	}

	// Start goroutine to check streaming status periodically
	go func() {
		ticker := time.NewTicker(checkInterval)
		defer ticker.Stop()

		// Track last known status to handle errors gracefully
		var lastKnownStatus bool

		// Function to check streaming status and send update
		checkAndUpdateStatus := func() {
			// Recover from any panics that might occur during API calls
			defer func() {
				if r := recover(); r != nil {
					log.Printf("Recovered from panic in YouTube API call: %v", r)
					// On panic, send the last known status if we've checked before
					select {
					case statusCh <- lastKnownStatus:
						log.Printf("Sent last known status (%v) after recovering from panic", lastKnownStatus)
					case <-ctx.Done():
						return
					default:
						// Non-blocking - if channel is full, just continue
					}
				}
			}()

			// Check for live broadcasts with timeout context
			apiCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
			defer cancel()

			searchResponse, err := youtubeService.LiveBroadcasts.List([]string{"snippet", "id"}).
				BroadcastStatus("active").
				Context(apiCtx).
				Do()

			if err != nil {
				log.Printf("Error checking live broadcasts: %v", err)
				// On error, send the last known status if we've checked before
				// This prevents the window from getting "stuck" due to API errors
				select {
				case statusCh <- lastKnownStatus:
					log.Printf("Sent last known status (%v) due to API error", lastKnownStatus)
				case <-ctx.Done():
					return
				default:
					// Non-blocking - if channel is full, just continue
				}

				// Add a small delay before the next check on error
				time.Sleep(1 * time.Second)
				return
			}

			isLive := len(searchResponse.Items) > 0
			lastKnownStatus = isLive // Update last known status

			if isLive {
				log.Printf("Stream is live: %s", searchResponse.Items[0].Snippet.Title)
			} else {
				log.Printf("No active stream found")
			}

			// Send the status to the channel
			select {
			case statusCh <- isLive:
			case <-ctx.Done():
				return
			}
		}

		// Perform an immediate check when starting
		checkAndUpdateStatus()

		for {
			select {
			case <-ticker.C:
				checkAndUpdateStatus()
			case <-ctx.Done():
				close(statusCh)
				return
			}
		}
	}()

	return statusCh
}
