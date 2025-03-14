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

		// Function to check streaming status and send update
		checkAndUpdateStatus := func() {
			// Check for live broadcasts with timeout context
			apiCtx, cancel := context.WithTimeout(ctx, 10*time.Second)
			defer cancel()

			searchResponse, err := youtubeService.LiveBroadcasts.List([]string{"snippet", "id"}).
				BroadcastStatus("active").
				Context(apiCtx).
				Do()
			if err != nil {
				log.Printf("Error checking live broadcasts: %v", err)

				// Add a small delay before the next check on error
				time.Sleep(1 * time.Second)
				return
			}

			isLive := len(searchResponse.Items) > 0

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
			case <-time.After(10 * time.Second):
				log.Println("timed out to send stream status to channel")
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
