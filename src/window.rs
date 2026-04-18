use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use log::{debug, error, info};
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT,
    },
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
};

// Frame color state, read by wndproc in the window thread and written by the audio task.
// 0 = unknown (defaults to red), 1 = red (silent), 2 = green (audible).
pub const COLOR_UNKNOWN: u8 = 0;
pub const COLOR_RED: u8 = 1;
pub const COLOR_GREEN: u8 = 2;

static COLOR_STATE: AtomicU8 = AtomicU8::new(COLOR_UNKNOWN);

// Updates the color state. If the category changed, invalidates the window so wndproc repaints.
pub fn set_color_state(hwnd: HWND, new_state: u8) {
    let prev = COLOR_STATE.swap(new_state, Ordering::Relaxed);
    if prev != new_state && hwnd.0 != 0 {
        unsafe {
            InvalidateRect(hwnd, None, TRUE);
        }
    }
}

// Function to create window and run message loop in a separate thread
pub unsafe fn create_window_and_run_message_loop(tx: mpsc::Sender<HWND>) -> Result<()> {
    // Register the window class
    debug!("Registering window class...");
    let instance = GetModuleHandleW(None)?;
    let window_class = w!("RedFrameWindowClass");

    let wc = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hCursor: LoadCursorW(None, IDC_ARROW)?,
        lpszClassName: window_class,
        ..Default::default()
    };

    RegisterClassExW(&wc);

    // Get the dimensions of the main display
    let screen_width = GetSystemMetrics(SM_CXSCREEN);
    let screen_height = GetSystemMetrics(SM_CYSCREEN);
    debug!("Screen dimensions: {}x{}", screen_width, screen_height);

    // Create the window with the specified styles
    info!("Creating window with red frame...");
    let hwnd = CreateWindowExW(
        WS_EX_TOOLWINDOW | WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
        window_class,
        w!("Red Frame"),
        WS_POPUP,
        0,             // X position (at the left edge of the screen)
        0,             // Y position (at the top edge of the screen)
        screen_width,  // Width (screen width)
        screen_height, // Height (screen height)
        None,
        None,
        instance,
        None,
    );

    if hwnd.0 == 0 {
        error!("Failed to create window");
        return Err(Error::from_win32());
    }

    // Send the window handle to the main thread
    if let Err(e) = tx.send(hwnd) {
        error!("Failed to send window handle: {}", e);
        return Err(Error::from_win32());
    }

    // Set the window to be transparent except for the red frame
    debug!("Setting window transparency...");
    let color_key = COLORREF(0); // Black is transparent
                                 // Use the full path for SetLayeredWindowAttributes
    SetLayeredWindowAttributes(hwnd, color_key, 255, LWA_COLORKEY);

    // Message loop
    info!("Starting window message loop...");
    let mut message = MSG::default();
    while GetMessageW(&mut message, None, 0, 0).into() {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }

    info!("Window message loop ended");
    Ok(())
}

extern "system" fn wndproc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match message {
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let hdc = BeginPaint(hwnd, &mut ps);

                // Get client area dimensions
                let mut rect = RECT::default();
                GetClientRect(hwnd, &mut rect);

                // COLORREF is 0x00BBGGRR. Green when audio is audible, red otherwise.
                let color = match COLOR_STATE.load(Ordering::Relaxed) {
                    COLOR_GREEN => COLORREF(0x00FF00),
                    _ => COLORREF(0x0000FF),
                };
                let brush = CreateSolidBrush(color);

                let frame_thickness = 3;

                let top_rect = RECT {
                    left: 0,
                    top: 0,
                    right: rect.right,
                    bottom: frame_thickness,
                };
                FillRect(hdc, &top_rect, brush);

                let bottom_rect = RECT {
                    left: 0,
                    top: rect.bottom - frame_thickness,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &bottom_rect, brush);

                let left_rect = RECT {
                    left: 0,
                    top: 0,
                    right: frame_thickness,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &left_rect, brush);

                let right_rect = RECT {
                    left: rect.right - frame_thickness,
                    top: 0,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &right_rect, brush);

                DeleteObject(brush);

                EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}

// Function to show or hide the window
pub unsafe fn set_window_visibility(hwnd: HWND, visible: bool) {
    if hwnd.0 != 0 {
        if visible {
            ShowWindow(hwnd, SW_SHOW);
            info!("Window shown (streaming active)");
        } else {
            ShowWindow(hwnd, SW_HIDE);
            info!("Window hidden (not streaming)");
        }
    }
}