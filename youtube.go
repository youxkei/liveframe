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

	// Get the authenticated user's channel ID
	var channelID string
	channelsResponse, err := youtubeService.Channels.List([]string{"id"}).Mine(true).Do()
	if err != nil {
		log.Fatalf("Error getting channel ID: %v", err)
	}

	if len(channelsResponse.Items) == 0 {
		log.Fatalf("Could not find authenticated user's channel")
	}

	channelID = channelsResponse.Items[0].Id
	log.Printf("Found channel ID: %s", channelID)

	// Start goroutine to check streaming status periodically
	go func() {
		ticker := time.NewTicker(checkInterval)
		defer ticker.Stop()

		for {
			select {
			case <-ticker.C:
				// Check for live broadcasts
				searchResponse, err := youtubeService.Search.List([]string{"id", "snippet"}).
					ChannelId(channelID).
					EventType("live").
					Type("video").
					Do()

				if err != nil {
					log.Printf("Error checking live broadcasts: %v", err)
					continue
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
				}

			case <-ctx.Done():
				close(statusCh)
				return
			}
		}
	}()

	return statusCh
}
