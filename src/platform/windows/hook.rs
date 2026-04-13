use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};

use crate::error::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookMode {
    Disabled,
    StandardHotkey,
    LowLevelKeyboard,
}

impl HookMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Disabled => "已禁用",
            Self::StandardHotkey => "标准轮询",
            Self::LowLevelKeyboard => "低级键盘钩子",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookStatus {
    pub mode: HookMode,
    pub installed: bool,
}

impl HookStatus {
    pub fn install(mode: HookMode) -> Result<Self, AppError> {
        if mode == HookMode::Disabled {
            return Err(AppError::HookInstallFailed(String::from(
                "不能安装已禁用的钩子模式",
            )));
        }

        Ok(Self {
            mode,
            installed: true,
        })
    }

    pub fn label(&self) -> &'static str {
        self.mode.label()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    KeyDown(u16),
    KeyUp(u16),
}

pub struct LowLevelKeyboardHook {
    receiver: Receiver<HookEvent>,
    thread_id: u32,
    join_handle: Option<JoinHandle<()>>,
}

impl LowLevelKeyboardHook {
    pub fn install() -> Result<Self, AppError> {
        #[cfg(windows)]
        {
            install_windows_hook()
        }

        #[cfg(not(windows))]
        {
            Err(AppError::Unsupported(String::from(
                "低级键盘钩子仅支持在 Windows 上使用",
            )))
        }
    }

    pub fn drain_events(&mut self) -> Result<Vec<HookEvent>, AppError> {
        let mut events = Vec::new();

        loop {
            match self.receiver.try_recv() {
                Ok(event) => events.push(event),
                Err(TryRecvError::Empty) => return Ok(events),
                Err(TryRecvError::Disconnected) => {
                    return Err(AppError::HookInstallFailed(String::from(
                        "键盘钩子线程已断开",
                    )));
                }
            }
        }
    }
}

impl Drop for LowLevelKeyboardHook {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};

            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }

        if let Some(join_handle) = self.join_handle.take() {
            let _ = join_handle.join();
        }
    }
}

#[cfg(windows)]
fn install_windows_hook() -> Result<LowLevelKeyboardHook, AppError> {
    let (event_tx, event_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();

    let join_handle = thread::spawn(move || run_hook_thread(event_tx, ready_tx));

    let thread_id = ready_rx.recv().map_err(|_| {
        AppError::HookInstallFailed(String::from("键盘钩子线程没有正确回报启动状态"))
    })??;

    Ok(LowLevelKeyboardHook {
        receiver: event_rx,
        thread_id,
        join_handle: Some(join_handle),
    })
}

#[cfg(windows)]
fn run_hook_thread(event_tx: Sender<HookEvent>, ready_tx: Sender<Result<u32, AppError>>) {
    use std::mem::MaybeUninit;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, PeekMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, MSG, PM_NOREMOVE, WH_KEYBOARD_LL,
    };

    if let Err(error) = set_global_sender(event_tx) {
        let _ = ready_tx.send(Err(error));
        return;
    }

    let mut message = MaybeUninit::<MSG>::zeroed();
    unsafe {
        PeekMessageW(message.as_mut_ptr(), null_mut(), 0, 0, PM_NOREMOVE);
    }

    let module = unsafe { GetModuleHandleW(null()) };
    if module == null_mut() {
        clear_global_sender();
        let _ = ready_tx.send(Err(AppError::HookInstallFailed(format!(
            "GetModuleHandleW 调用失败：{}",
            std::io::Error::last_os_error()
        ))));
        return;
    }

    let hook =
        unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard_proc), module, 0) };
    if hook == null_mut() {
        clear_global_sender();
        let _ = ready_tx.send(Err(AppError::HookInstallFailed(format!(
            "SetWindowsHookExW 调用失败：{}",
            std::io::Error::last_os_error()
        ))));
        return;
    }

    let thread_id = unsafe { GetCurrentThreadId() };
    let _ = ready_tx.send(Ok(thread_id));

    loop {
        let result = unsafe { GetMessageW(message.as_mut_ptr(), null_mut(), 0, 0) };
        if result <= 0 {
            break;
        }

        unsafe {
            TranslateMessage(message.as_ptr());
            DispatchMessageW(message.as_ptr());
        }
    }

    unsafe {
        UnhookWindowsHookEx(hook);
    }
    clear_global_sender();
}

#[cfg(windows)]
fn hook_sender_slot() -> &'static Mutex<Option<Sender<HookEvent>>> {
    static HOOK_SENDER: OnceLock<Mutex<Option<Sender<HookEvent>>>> = OnceLock::new();
    HOOK_SENDER.get_or_init(|| Mutex::new(None))
}

#[cfg(windows)]
fn set_global_sender(sender: Sender<HookEvent>) -> Result<(), AppError> {
    let mut guard = hook_sender_slot()
        .lock()
        .map_err(|_| AppError::HookInstallFailed(String::from("键盘钩子发送器状态已损坏")))?;

    if guard.is_some() {
        return Err(AppError::HookInstallFailed(String::from(
            "当前进程已经安装了键盘钩子",
        )));
    }

    *guard = Some(sender);
    Ok(())
}

#[cfg(windows)]
fn clear_global_sender() {
    if let Ok(mut guard) = hook_sender_slot().lock() {
        *guard = None;
    }
}

#[cfg(windows)]
unsafe extern "system" fn low_level_keyboard_proc(
    code: i32,
    wparam: usize,
    lparam: isize,
) -> isize {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    };

    if code >= 0 && lparam != 0 {
        let keyboard = &*(lparam as *const KBDLLHOOKSTRUCT);
        let event = match wparam as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(HookEvent::KeyDown(keyboard.vkCode as u16)),
            WM_KEYUP | WM_SYSKEYUP => Some(HookEvent::KeyUp(keyboard.vkCode as u16)),
            _ => None,
        };

        if let Some(event) = event {
            if let Ok(guard) = hook_sender_slot().lock() {
                if let Some(sender) = guard.as_ref() {
                    let _ = sender.send(event);
                }
            }
        }
    }

    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}
