package main

import (
	"context"
	"log"
	"os"
	"os/signal"
	"path/filepath"
	"syscall"
	"time"

	"github.com/lxn/win"
)

func main() {
	log.Println("Starting LiveFrame - YouTube Streaming Border")

	// Create root context
	ctx := context.Background()

	// Set up signal handling for graceful shutdown
	ctx, stop := signal.NotifyContext(ctx, os.Interrupt, syscall.SIGTERM)
	defer stop()

	// Set up OAuth authentication
	home, err := os.UserHomeDir()
	if err != nil {
		log.Fatalf("failed to get home directory: %v", err)
	}
	secretFile := filepath.Join(home, ".liveframe", "secret.json")

	// Load client secret
	config, err := LoadClientSecretFromFile(secretFile)
	if err != nil {
		log.Fatalf("Error loading client secret: %v", err)
	}

	// Get OAuth client
	client, err := GetOAuthClient(ctx, config)
	if err != nil {
		log.Fatalf("Error getting OAuth client: %v", err)
	}

	log.Println("OAuth authentication successful")

	// Create border window
	_, windowManager, err := CreateBorderWindow(ctx)
	if err != nil {
		log.Fatalf("Error creating window: %v", err)
	}

	// Set up streaming status check
	log.Println("Setting up YouTube streaming status check")
	statusCh := IsLiveStreaming(ctx, client, 5*time.Second)

	// Handle streaming status updates with recovery mechanism
	go func() {
		for {
			select {
			case isLive, ok := <-statusCh:
				if !ok {
					log.Println("Status channel closed, exiting status handler")
					// Channel closed, context is done
					return
				}

				// Log status change
				log.Printf("Received streaming status update: isLive=%v", isLive)
				windowManager.SetVisible(isLive)

			case <-ctx.Done():
				log.Println("Context done, exiting status handler")
				return

			// Add a timeout case to ensure the goroutine doesn't hang indefinitely
			case <-time.After(30 * time.Second):
				// This is just a safety check to ensure the select doesn't block forever
				// if both the channel and context somehow get stuck
				log.Println("keep alive for stream status check")

				continue
			}
		}
	}()

	// Message loop - runs until WM_QUIT is received
	var msg win.MSG

	// Main event loop
	for {
		// Check if context is done or process Windows messages
		select {
		case <-ctx.Done():
			log.Println("Context canceled, exiting...")
			win.PostQuitMessage(0)
			return

		default:
			// Process Windows messages using PeekMessage
			if win.PeekMessage(&msg, 0, 0, 0, win.PM_REMOVE) {
				if msg.Message == win.WM_QUIT {
					log.Println("Received WM_QUIT, exiting...")
					return
				}

				// Handle Windows messages
				win.TranslateMessage(&msg)
				win.DispatchMessage(&msg)
			} else {
				// Small sleep to prevent CPU from maxing out
				// Use a shorter sleep time for better responsiveness
				time.Sleep(5 * time.Millisecond)
			}
		}
	}
}
