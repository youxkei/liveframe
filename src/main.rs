use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Graphics::Gdi::{
        BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, PAINTSTRUCT,
    },
    Win32::System::LibraryLoader::GetModuleHandleW,
    Win32::UI::WindowsAndMessaging::*,
};

// Import COLORREF from Foundation
use windows::Win32::Foundation::COLORREF;
// Import GetClientRect from WindowsAndMessaging
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

fn main() -> Result<()> {
    unsafe {
        // Register the window class
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

        // Create the window with the specified styles
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
            return Err(Error::from_win32());
        }
        // Set the window to be transparent except for the red frame
        let color_key = COLORREF(0); // Black is transparent
                                     // Use the full path for SetLayeredWindowAttributes
        windows::Win32::UI::WindowsAndMessaging::SetLayeredWindowAttributes(
            hwnd,
            color_key,
            255,
            LWA_COLORKEY,
        );

        // Show the window
        ShowWindow(hwnd, SW_SHOW);

        // Message loop
        let mut message = MSG::default();
        while GetMessageW(&mut message, None, 0, 0).into() {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        Ok(())
    }
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

                // Create a red brush for the frame
                let red_brush = CreateSolidBrush(COLORREF(0x0000FF)); // RGB format is 0x00BBGGRR

                // Frame thickness (3 pixels)
                let frame_thickness = 3;

                // Draw the frame exactly at the edges of the screen
                // Top frame
                let top_rect = RECT {
                    left: 0,
                    top: 0,
                    right: rect.right,
                    bottom: frame_thickness,
                };
                FillRect(hdc, &top_rect, red_brush);

                // Bottom frame
                let bottom_rect = RECT {
                    left: 0,
                    top: rect.bottom - frame_thickness,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &bottom_rect, red_brush);

                // Left frame
                let left_rect = RECT {
                    left: 0,
                    top: 0,
                    right: frame_thickness,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &left_rect, red_brush);

                // Right frame
                let right_rect = RECT {
                    left: rect.right - frame_thickness,
                    top: 0,
                    right: rect.right,
                    bottom: rect.bottom,
                };
                FillRect(hdc, &right_rect, red_brush);

                // Clean up
                DeleteObject(red_brush);

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
