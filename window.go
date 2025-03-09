package main

import (
	"context"
	"fmt"
	"log"
	"sync"
	"syscall"
	"unsafe"

	"github.com/lxn/win"
)

const (
	borderWidth = 2 // Border width in pixels
)

var (
	className  = mustGetUTF16PtrFromString("RedBorderWindow")
	windowName = mustGetUTF16PtrFromString("LiveFrame - YouTube Streaming Border")

	idcArrow = mustGetUTF16PtrFromString("IDC_ARROW")
)

func mustGetUTF16PtrFromString(str string) *uint16 {
	ptr, err := syscall.UTF16PtrFromString(str)
	if err != nil {
		panic(fmt.Sprintf("failed to convert string %q to UTF16 pointer", str))
	}

	return ptr
}

// WindowManager manages the window visibility
type WindowManager struct {
	hwnd win.HWND
	mu   sync.Mutex
}

// NewWindowManager creates a new window manager
func NewWindowManager(hwnd win.HWND) *WindowManager {
	return &WindowManager{
		hwnd: hwnd,
	}
}

// SetVisible sets the window visibility
func (wm *WindowManager) SetVisible(visible bool) {
	if wm == nil {
		log.Println("Warning: windowManager is nil, cannot update visibility")
		return
	}

	wm.mu.Lock()
	defer wm.mu.Unlock()

	// Check if window handle is valid
	if wm.hwnd == 0 {
		log.Println("Warning: Invalid window handle in SetVisible")
		return
	}

	if visible {
		win.ShowWindow(wm.hwnd, win.SW_SHOW)
		win.UpdateWindow(wm.hwnd)
		log.Println("Window is now visible - YouTube stream detected")
	} else {
		win.ShowWindow(wm.hwnd, win.SW_HIDE)
		log.Println("Window is now hidden - No active YouTube stream")
	}
}

// CreateBorderWindow creates the border window that will be shown during streaming
func CreateBorderWindow(ctx context.Context) (win.HWND, *WindowManager, error) {
	// Register window class

	hInstance := win.GetModuleHandle(nil)

	var icex win.INITCOMMONCONTROLSEX
	icex.DwSize = uint32(unsafe.Sizeof(icex))
	icex.DwICC = win.ICC_STANDARD_CLASSES
	win.InitCommonControlsEx(&icex)

	wcex := win.WNDCLASSEX{
		CbSize:        uint32(unsafe.Sizeof(win.WNDCLASSEX{})),
		Style:         win.CS_HREDRAW | win.CS_VREDRAW,
		LpfnWndProc:   syscall.NewCallback(wndProc),
		HInstance:     hInstance,
		HCursor:       win.LoadCursor(0, idcArrow),
		HbrBackground: win.HBRUSH(win.GetStockObject(win.BLACK_BRUSH)),
		LpszClassName: className,
	}

	if atom := win.RegisterClassEx(&wcex); atom == 0 {
		return 0, nil, fmt.Errorf("RegisterClassEx failed")
	}

	// Get screen dimensions
	screenWidth := int32(win.GetSystemMetrics(win.SM_CXSCREEN))
	screenHeight := int32(win.GetSystemMetrics(win.SM_CYSCREEN))

	hwnd := win.CreateWindowEx(
		WS_EX_LAYERED|WS_EX_TOPMOST|WS_EX_NOACTIVATE,
		className,
		windowName,
		win.WS_POPUP,
		0, 0, screenWidth, screenHeight,
		0, 0, hInstance, nil,
	)

	if hwnd == 0 {
		return 0, nil, fmt.Errorf("CreateWindowEx failed")
	}

	// Make window transparent except for the border
	win.SetWindowLong(hwnd, win.GWL_EXSTYLE, win.GetWindowLong(hwnd, win.GWL_EXSTYLE)|WS_EX_LAYERED)

	// Make window transparent
	if !SetLayeredWindowAttributes(hwnd, 0, 0, LWA_COLORKEY) {
		return 0, nil, fmt.Errorf("SetLayeredWindowAttributes failed")
	}

	// Create window manager (initially hidden)
	windowManager := NewWindowManager(hwnd)
	windowManager.SetVisible(false)

	// Clean up when context is done
	go func() {
		<-ctx.Done()

		log.Printf("destroying window due to context cancel")
		win.DestroyWindow(hwnd)
	}()

	return hwnd, windowManager, nil
}

func drawRedBorder(hwnd win.HWND) {
	var rc win.RECT
	win.GetClientRect(hwnd, &rc)

	hdc := win.GetDC(hwnd)
	defer win.ReleaseDC(hwnd, hdc)

	// Create a red brush
	redBrush := CreateSolidBrush(win.RGB(255, 0, 0))
	defer win.DeleteObject(win.HGDIOBJ(redBrush))

	// Select the brush into the DC
	oldBrush := win.SelectObject(hdc, win.HGDIOBJ(redBrush))
	defer win.SelectObject(hdc, oldBrush)

	// Draw the top border
	PatBlt(hdc, 0, 0, int(rc.Right), borderWidth, PATCOPY)

	// Draw the bottom border
	PatBlt(hdc, 0, int(rc.Bottom-borderWidth), int(rc.Right), borderWidth, PATCOPY)

	// Draw the left border
	PatBlt(hdc, 0, 0, borderWidth, int(rc.Bottom), PATCOPY)

	// Draw the right border
	PatBlt(hdc, int(rc.Right-borderWidth), 0, borderWidth, int(rc.Bottom), PATCOPY)
}

func wndProc(hwnd win.HWND, msg uint32, wParam, lParam uintptr) uintptr {
	switch msg {
	case win.WM_DESTROY:
		win.PostQuitMessage(0)
		return 0

	case win.WM_KEYDOWN:
		// Close on ESC key
		if wParam == win.VK_ESCAPE {
			win.DestroyWindow(hwnd)
		}
		return 0

	case win.WM_PAINT:
		var ps win.PAINTSTRUCT
		win.BeginPaint(hwnd, &ps)
		drawRedBorder(hwnd)
		win.EndPaint(hwnd, &ps)
		return 0
	}

	return win.DefWindowProc(hwnd, msg, wParam, lParam)
}
