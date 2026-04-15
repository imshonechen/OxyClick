#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForegroundWindow {
    pub handle: isize,
    pub process_id: u32,
}

impl ForegroundWindow {
    pub fn belongs_to_current_process(self) -> bool {
        self.process_id == std::process::id()
    }
}

pub fn disable_raw_mouse_device_events() -> bool {
    #[cfg(windows)]
    {
        use std::{mem::size_of, ptr};
        use windows_sys::Win32::UI::Input::{
            RegisterRawInputDevices, RAWINPUTDEVICE, RIDEV_REMOVE,
        };

        const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
        const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

        let device = RAWINPUTDEVICE {
            usUsagePage: HID_USAGE_PAGE_GENERIC,
            usUsage: HID_USAGE_GENERIC_MOUSE,
            dwFlags: RIDEV_REMOVE,
            hwndTarget: ptr::null_mut(),
        };

        unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 }
    }

    #[cfg(not(windows))]
    {
        false
    }
}

pub fn current_foreground_window() -> Option<ForegroundWindow> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };

        let handle = unsafe { GetForegroundWindow() };
        if handle.is_null() {
            return None;
        }

        let mut process_id = 0_u32;
        unsafe {
            GetWindowThreadProcessId(handle, &mut process_id);
        }

        Some(ForegroundWindow {
            handle: handle as isize,
            process_id,
        })
    }

    #[cfg(not(windows))]
    {
        None
    }
}
