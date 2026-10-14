#[test]
fn windows_host_supports_chromium_layered_child_surfaces() {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::{
        Foundation::GetLastError,
        UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WS_CHILD, WS_EX_LAYERED},
    };

    let class = "STATIC\0".encode_utf16().collect::<Vec<_>>();
    // SAFETY: The builtin class is valid, and both invisible windows are owned by this test thread.
    unsafe {
        let parent = CreateWindowExW(
            0,
            class.as_ptr(),
            null(),
            0,
            0,
            0,
            1,
            1,
            null_mut(),
            null_mut(),
            null_mut(),
            null(),
        );
        assert!(
            !parent.is_null(),
            "Native parent creation failed: {}",
            GetLastError()
        );
        // Chromium's DirectComposition compositor requires layered child windows.
        let child = CreateWindowExW(
            WS_EX_LAYERED,
            class.as_ptr(),
            null(),
            WS_CHILD,
            0,
            0,
            1,
            1,
            parent,
            null_mut(),
            null_mut(),
            null(),
        );
        let error = GetLastError();
        if !child.is_null() {
            let _ = DestroyWindow(child);
        }
        let _ = DestroyWindow(parent);
        assert!(
            !child.is_null(),
            "Chromium layered child creation failed: Win32 error {error}; check Windows compatibility in the executable manifest"
        );
    }
}
