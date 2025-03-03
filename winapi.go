package main

import (
	"fmt"
	"unsafe"

	"github.com/lxn/win"
	"golang.org/x/sys/windows"
)

// Windows API constants not defined in lxn/win
const (
	WS_EX_LAYERED    = 0x00080000
	WS_EX_TOPMOST    = 0x00000008
	WS_EX_NOACTIVATE = 0x08000000 // Prevents window from becoming active
	LWA_COLORKEY     = 0x00000001
	PATCOPY          = 0x00F00021
)

var (
	// Import required functions from user32.dll
	user32                         = windows.NewLazyDLL("user32.dll")
	procSetLayeredWindowAttributes = user32.NewProc("SetLayeredWindowAttributes")

	// Import required functions from gdi32.dll
	gdi32                = windows.NewLazyDLL("gdi32.dll")
	procCreateSolidBrush = gdi32.NewProc("CreateSolidBrush")
	procPatBlt           = gdi32.NewProc("PatBlt")
)

// SetLayeredWindowAttributes wraps the Windows API function
func SetLayeredWindowAttributes(hwnd win.HWND, crKey win.COLORREF, bAlpha byte, dwFlags uint32) bool {
	ret, _, _ := procSetLayeredWindowAttributes.Call(
		uintptr(hwnd),
		uintptr(crKey),
		uintptr(bAlpha),
		uintptr(dwFlags),
	)
	return ret != 0
}

// CreateSolidBrush wraps the Windows API function
func CreateSolidBrush(color win.COLORREF) win.HBRUSH {
	ret, _, _ := procCreateSolidBrush.Call(uintptr(color))
	return win.HBRUSH(ret)
}

// PatBlt wraps the Windows API function
func PatBlt(hdc win.HDC, x, y, width, height int, rop uint32) bool {
	ret, _, _ := procPatBlt.Call(
		uintptr(hdc),
		uintptr(x),
		uintptr(y),
		uintptr(width),
		uintptr(height),
		uintptr(rop),
	)
	return ret != 0
}

// ShellExecute wraps the Windows API function to open URLs
var shell32 = windows.NewLazyDLL("shell32.dll")
var procShellExecute = shell32.NewProc("ShellExecuteW")

// OpenURL opens a URL in the default browser
func OpenURL(url string) error {
	verb := "open"
	lpFile := url

	verbPtr, err := windows.UTF16PtrFromString(verb)
	if err != nil {
		return fmt.Errorf("failed to convert verb to UTF16: %w", err)
	}

	lpFilePtr, err := windows.UTF16PtrFromString(lpFile)
	if err != nil {
		return fmt.Errorf("failed to convert URL to UTF16: %w", err)
	}

	ret, _, _ := procShellExecute.Call(
		uintptr(0),
		uintptr(unsafe.Pointer(verbPtr)),
		uintptr(unsafe.Pointer(lpFilePtr)),
		uintptr(0),
		uintptr(0),
		uintptr(1), // SW_SHOWNORMAL
	)

	if ret <= 32 {
		return fmt.Errorf("failed to open URL: %w", windows.GetLastError())
	}
	return nil
}
