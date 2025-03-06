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
	statusCh := make(chan bool)

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
			// Check for live broadcasts
			searchResponse, err := youtubeService.LiveBroadcasts.List([]string{"snippet", "id"}).BroadcastStatus("active").Do()

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
