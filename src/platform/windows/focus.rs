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
