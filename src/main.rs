#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

fn main() {
    if let Err(error) = oxyclick::app::run() {
        report_startup_error(&format!("OxyClick 启动失败：{error}"));
        std::process::exit(1);
    }
}

fn report_startup_error(message: &str) {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::iter;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

        let title: Vec<u16> = OsStr::new("OxyClick")
            .encode_wide()
            .chain(iter::once(0))
            .collect();
        let content: Vec<u16> = OsStr::new(message)
            .encode_wide()
            .chain(iter::once(0))
            .collect();

        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                content.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR,
            );
        }
    }

    #[cfg(not(windows))]
    {
        eprintln!("{message}");
    }
}
