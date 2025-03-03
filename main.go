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
	statusCh := IsLiveStreaming(ctx, client, 30*time.Second)

	// Handle streaming status updates
	go func() {
		for {
			select {
			case isLive, ok := <-statusCh:
				if !ok {
					// Channel closed, context is done
					return
				}
				windowManager.SetVisible(isLive)
			case <-ctx.Done():
				return
			}
		}
	}()

	// Message loop - runs until WM_QUIT is received
	var msg win.MSG
	for {
		// Check if context is done
		select {
		case <-ctx.Done():
			log.Println("Context canceled, exiting...")
			win.PostQuitMessage(0)
			return
		default:
			// Process Windows messages using PeekMessage instead of GetMessage
			// This allows us to check for context cancellation more frequently
			if win.PeekMessage(&msg, 0, 0, 0, win.PM_REMOVE) {
				if msg.Message == win.WM_QUIT {
					return
				}
				win.TranslateMessage(&msg)
				win.DispatchMessage(&msg)
			}
			// Small sleep to prevent CPU from maxing out
			time.Sleep(10 * time.Millisecond)
		}
	}
}
