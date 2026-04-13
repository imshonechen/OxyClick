use std::collections::BTreeSet;
use std::thread;
use std::time::Duration;

use crate::core::model::{InputAction, MouseButton};
use crate::error::AppError;

pub trait InputBackend {
    fn send_action(&mut self, action: &InputAction, press_duration_ms: u64)
        -> Result<(), AppError>;
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NoopInputBackend {
    sent_actions: u64,
}

impl NoopInputBackend {
    pub fn sent_actions(&self) -> u64 {
        self.sent_actions
    }
}

impl InputBackend for NoopInputBackend {
    fn send_action(
        &mut self,
        _action: &InputAction,
        _press_duration_ms: u64,
    ) -> Result<(), AppError> {
        self.sent_actions += 1;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendMode {
    SendInput,
    Noop,
}

impl BackendMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SendInput => "系统输入注入（SendInput）",
            Self::Noop => "模拟回退模式",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsInputBackend {
    mode: BackendMode,
    noop: NoopInputBackend,
}

impl WindowsInputBackend {
    pub fn detect() -> Self {
        #[cfg(windows)]
        {
            Self {
                mode: BackendMode::SendInput,
                noop: NoopInputBackend::default(),
            }
        }

        #[cfg(not(windows))]
        {
            Self::noop()
        }
    }

    pub fn noop() -> Self {
        Self {
            mode: BackendMode::Noop,
            noop: NoopInputBackend::default(),
        }
    }

    pub fn mode(&self) -> BackendMode {
        self.mode
    }
}

impl Default for WindowsInputBackend {
    fn default() -> Self {
        Self::detect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyCapture {
    pub label: String,
}

impl InputBackend for WindowsInputBackend {
    fn send_action(
        &mut self,
        action: &InputAction,
        press_duration_ms: u64,
    ) -> Result<(), AppError> {
        match self.mode {
            BackendMode::Noop => self.noop.send_action(action, press_duration_ms),
            BackendMode::SendInput => send_windows_action(action, press_duration_ms),
        }
    }
}

#[cfg(windows)]
fn send_windows_action(action: &InputAction, press_duration_ms: u64) -> Result<(), AppError> {
    match action {
        InputAction::MouseClick { button } => send_mouse_click(*button),
        InputAction::KeyPress { key_code } => send_key_press(key_code, press_duration_ms),
        InputAction::KeyCombo {
            modifiers,
            key_code,
        } => send_key_combo(modifiers, key_code, press_duration_ms),
    }
}

#[cfg(not(windows))]
fn send_windows_action(_action: &InputAction, _press_duration_ms: u64) -> Result<(), AppError> {
    Err(AppError::Unsupported(String::from(
        "SendInput 后端仅支持在 Windows 上使用",
    )))
}

#[cfg(windows)]
fn send_mouse_click(button: MouseButton) -> Result<(), AppError> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
        MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEINPUT,
    };

    let (down_flag, up_flag) = match button {
        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
    };

    let mut inputs = [
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: down_flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: up_flag,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    send_inputs(&mut inputs)
}

#[cfg(windows)]
fn send_key_press(key_code: &str, press_duration_ms: u64) -> Result<(), AppError> {
    let vk = parse_virtual_key(key_code)
        .ok_or_else(|| AppError::InputSendFailed(format!("不支持的按键代码：{key_code}")))?;

    send_key_sequence(&[vk], vk, press_duration_ms)
}

#[cfg(windows)]
fn send_key_combo(
    modifiers: &[String],
    key_code: &str,
    press_duration_ms: u64,
) -> Result<(), AppError> {
    let mut modifier_codes = Vec::with_capacity(modifiers.len());
    for modifier in modifiers {
        let code = parse_virtual_key(modifier)
            .ok_or_else(|| AppError::InputSendFailed(format!("不支持的修饰键：{modifier}")))?;
        modifier_codes.push(code);
    }

    let vk = parse_virtual_key(key_code)
        .ok_or_else(|| AppError::InputSendFailed(format!("不支持的按键代码：{key_code}")))?;

    send_key_sequence(&modifier_codes, vk, press_duration_ms)
}

#[cfg(windows)]
fn send_key_sequence(
    modifiers: &[u16],
    primary_key: u16,
    press_duration_ms: u64,
) -> Result<(), AppError> {
    let mut down_inputs = Vec::with_capacity(modifiers.len() + 1);
    for modifier in modifiers {
        down_inputs.push(key_input(*modifier, 0));
    }
    down_inputs.push(key_input(primary_key, 0));
    send_inputs(&mut down_inputs)?;

    if press_duration_ms > 0 {
        thread::sleep(Duration::from_millis(press_duration_ms));
    }

    let mut up_inputs = Vec::with_capacity(modifiers.len() + 1);
    up_inputs.push(key_input(
        primary_key,
        windows_sys::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_KEYUP,
    ));
    for modifier in modifiers.iter().rev() {
        up_inputs.push(key_input(
            *modifier,
            windows_sys::Win32::UI::Input::KeyboardAndMouse::KEYEVENTF_KEYUP,
        ));
    }
    send_inputs(&mut up_inputs)
}

#[cfg(windows)]
fn key_input(
    virtual_key: u16,
    flags: windows_sys::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS,
) -> windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
    };

    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: virtual_key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

#[cfg(windows)]
fn send_inputs(
    inputs: &mut [windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT],
) -> Result<(), AppError> {
    use std::mem::size_of;

    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SendInput;

    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_mut_ptr(),
            size_of::<windows_sys::Win32::UI::Input::KeyboardAndMouse::INPUT>() as i32,
        )
    };

    if sent != inputs.len() as u32 {
        return Err(AppError::InputSendFailed(format!(
            "SendInput 仅成功注入了 {sent}/{} 个输入事件",
            inputs.len()
        )));
    }

    Ok(())
}

pub fn poll_hotkey_capture() -> Option<HotkeyCapture> {
    #[cfg(windows)]
    {
        poll_windows_hotkey_capture()
    }

    #[cfg(not(windows))]
    {
        None
    }
}

pub fn is_hotkey_virtual_key_down(virtual_key: u16) -> bool {
    #[cfg(windows)]
    {
        match virtual_key {
            0x10 => is_any_virtual_key_down(&[0x10, 0xA0, 0xA1]),
            0x11 => is_any_virtual_key_down(&[0x11, 0xA2, 0xA3]),
            0x12 => is_any_virtual_key_down(&[0x12, 0xA4, 0xA5]),
            0x5B => is_any_virtual_key_down(&[0x5B, 0x5C]),
            _ => is_virtual_key_down(virtual_key),
        }
    }

    #[cfg(not(windows))]
    {
        let _ = virtual_key;
        false
    }
}

pub fn pressed_keys_contain_virtual_key(pressed_keys: &BTreeSet<u16>, virtual_key: u16) -> bool {
    match virtual_key {
        0x10 => [0x10, 0xA0, 0xA1]
            .iter()
            .any(|candidate| pressed_keys.contains(candidate)),
        0x11 => [0x11, 0xA2, 0xA3]
            .iter()
            .any(|candidate| pressed_keys.contains(candidate)),
        0x12 => [0x12, 0xA4, 0xA5]
            .iter()
            .any(|candidate| pressed_keys.contains(candidate)),
        0x5B => [0x5B, 0x5C]
            .iter()
            .any(|candidate| pressed_keys.contains(candidate)),
        _ => pressed_keys.contains(&virtual_key),
    }
}

fn canonical_hotkey_label(parts: &[u16]) -> String {
    parts
        .iter()
        .filter_map(|virtual_key| canonical_virtual_key_name(*virtual_key))
        .collect::<Vec<_>>()
        .join("+")
}

fn canonical_virtual_key_name(virtual_key: u16) -> Option<String> {
    match virtual_key {
        0x08 => Some(String::from("Backspace")),
        0x09 => Some(String::from("Tab")),
        0x0D => Some(String::from("Enter")),
        0x10 => Some(String::from("Shift")),
        0x11 => Some(String::from("Ctrl")),
        0x12 => Some(String::from("Alt")),
        0x13 => Some(String::from("Pause")),
        0x1B => Some(String::from("Esc")),
        0x20 => Some(String::from("Space")),
        0x21 => Some(String::from("PageUp")),
        0x22 => Some(String::from("PageDown")),
        0x23 => Some(String::from("End")),
        0x24 => Some(String::from("Home")),
        0x25 => Some(String::from("Left")),
        0x26 => Some(String::from("Up")),
        0x27 => Some(String::from("Right")),
        0x28 => Some(String::from("Down")),
        0x2C => Some(String::from("PrintScreen")),
        0x2D => Some(String::from("Insert")),
        0x2E => Some(String::from("Delete")),
        0x5B => Some(String::from("Win")),
        0x70 => Some(String::from("F1")),
        0x71 => Some(String::from("F2")),
        0x72 => Some(String::from("F3")),
        0x73 => Some(String::from("F4")),
        0x74 => Some(String::from("F5")),
        0x75 => Some(String::from("F6")),
        0x76 => Some(String::from("F7")),
        0x77 => Some(String::from("F8")),
        0x78 => Some(String::from("F9")),
        0x79 => Some(String::from("F10")),
        0x7A => Some(String::from("F11")),
        0x7B => Some(String::from("F12")),
        0x7C => Some(String::from("F13")),
        0x7D => Some(String::from("F14")),
        0x7E => Some(String::from("F15")),
        0x7F => Some(String::from("F16")),
        0x80 => Some(String::from("F17")),
        0x81 => Some(String::from("F18")),
        0x82 => Some(String::from("F19")),
        0x83 => Some(String::from("F20")),
        0x84 => Some(String::from("F21")),
        0x85 => Some(String::from("F22")),
        0x86 => Some(String::from("F23")),
        0x87 => Some(String::from("F24")),
        0x30..=0x39 | 0x41..=0x5A => Some(char::from_u32(u32::from(virtual_key))?.to_string()),
        _ => None,
    }
}

#[cfg(windows)]
fn poll_windows_hotkey_capture() -> Option<HotkeyCapture> {
    let mut keys = Vec::new();

    if is_any_virtual_key_down(&[0xA2, 0xA3]) {
        keys.push(0x11);
    }
    if is_any_virtual_key_down(&[0xA4, 0xA5]) {
        keys.push(0x12);
    }
    if is_any_virtual_key_down(&[0xA0, 0xA1]) {
        keys.push(0x10);
    }
    if is_any_virtual_key_down(&[0x5B, 0x5C]) {
        keys.push(0x5B);
    }

    for virtual_key in hotkey_capture_scan_keys() {
        if is_virtual_key_down(*virtual_key) {
            keys.push(*virtual_key);
        }
    }

    if keys.is_empty() {
        return None;
    }

    Some(HotkeyCapture {
        label: canonical_hotkey_label(&keys),
    })
}

#[cfg(windows)]
fn hotkey_capture_scan_keys() -> &'static [u16] {
    &[
        0x08, 0x09, 0x0D, 0x13, 0x1B, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x2C,
        0x2D, 0x2E, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x41, 0x42, 0x43,
        0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x4B, 0x4C, 0x4D, 0x4E, 0x4F, 0x50, 0x51, 0x52,
        0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5A, 0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76,
        0x77, 0x78, 0x79, 0x7A, 0x7B, 0x7C, 0x7D, 0x7E, 0x7F, 0x80, 0x81, 0x82, 0x83, 0x84, 0x85,
        0x86, 0x87,
    ]
}

#[cfg(windows)]
fn is_any_virtual_key_down(virtual_keys: &[u16]) -> bool {
    virtual_keys
        .iter()
        .any(|virtual_key| is_virtual_key_down(*virtual_key))
}

#[cfg(windows)]
fn is_virtual_key_down(virtual_key: u16) -> bool {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;

    unsafe { (GetAsyncKeyState(i32::from(virtual_key)) as u16 & 0x8000) != 0 }
}

pub fn parse_virtual_key(name: &str) -> Option<u16> {
    let normalized = name.trim().to_ascii_uppercase();
    if normalized.len() == 1 {
        let character = normalized.as_bytes()[0];
        if character.is_ascii_uppercase() || character.is_ascii_digit() {
            return Some(u16::from(character));
        }
    }

    match normalized.as_str() {
        "SPACE" | "空格" | "空格键" => Some(0x20),
        "ENTER" | "RETURN" | "回车" | "回车键" => Some(0x0D),
        "TAB" | "制表" | "制表键" => Some(0x09),
        "ESC" | "ESCAPE" | "退出" => Some(0x1B),
        "BACKSPACE" | "BKSP" | "退格" | "退格键" => Some(0x08),
        "DELETE" | "DEL" | "删除" => Some(0x2E),
        "INSERT" | "INS" | "插入" => Some(0x2D),
        "HOME" | "首页" => Some(0x24),
        "END" | "结束" | "末尾" => Some(0x23),
        "PAGEUP" | "PGUP" | "上页" => Some(0x21),
        "PAGEDOWN" | "PGDN" | "下页" => Some(0x22),
        "LEFT" | "左" => Some(0x25),
        "UP" | "上" => Some(0x26),
        "RIGHT" | "右" => Some(0x27),
        "DOWN" | "下" => Some(0x28),
        "SHIFT" | "上档" | "换挡" => Some(0x10),
        "CTRL" | "CONTROL" | "控制" => Some(0x11),
        "ALT" | "MENU" | "替代" => Some(0x12),
        "CAPSLOCK" | "大写锁定" => Some(0x14),
        "PAUSE" | "暂停" => Some(0x13),
        "PRINTSCREEN" | "PRTSC" | "截图" | "打印屏幕" => Some(0x2C),
        "LWIN" | "WIN" | "META" | "左WIN" | "窗口" | "左窗口" => Some(0x5B),
        "RWIN" | "右WIN" | "右窗口" => Some(0x5C),
        _ => parse_function_key(&normalized),
    }
}

fn parse_function_key(name: &str) -> Option<u16> {
    let suffix = name.strip_prefix('F')?;
    let index: u16 = suffix.parse().ok()?;
    if (1..=24).contains(&index) {
        Some(0x70 + index - 1)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{parse_virtual_key, pressed_keys_contain_virtual_key};

    #[test]
    fn parses_alpha_keys() {
        assert_eq!(parse_virtual_key("A"), Some(0x41));
        assert_eq!(parse_virtual_key("z"), Some(0x5A));
    }

    #[test]
    fn parses_named_keys() {
        assert_eq!(parse_virtual_key("Space"), Some(0x20));
        assert_eq!(parse_virtual_key("Esc"), Some(0x1B));
        assert_eq!(parse_virtual_key("Ctrl"), Some(0x11));
    }

    #[test]
    fn parses_chinese_named_keys() {
        assert_eq!(parse_virtual_key("空格"), Some(0x20));
        assert_eq!(parse_virtual_key("回车"), Some(0x0D));
        assert_eq!(parse_virtual_key("控制"), Some(0x11));
        assert_eq!(parse_virtual_key("暂停"), Some(0x13));
        assert_eq!(parse_virtual_key("左Win"), Some(0x5B));
    }

    #[test]
    fn parses_function_keys() {
        assert_eq!(parse_virtual_key("F1"), Some(0x70));
        assert_eq!(parse_virtual_key("F12"), Some(0x7B));
        assert_eq!(parse_virtual_key("F25"), None);
    }

    #[test]
    fn generic_modifiers_match_specific_pressed_keys() {
        let mut pressed_keys = BTreeSet::new();
        pressed_keys.insert(0xA2);
        pressed_keys.insert(0xA5);
        pressed_keys.insert(0x5C);

        assert!(pressed_keys_contain_virtual_key(&pressed_keys, 0x11));
        assert!(pressed_keys_contain_virtual_key(&pressed_keys, 0x12));
        assert!(pressed_keys_contain_virtual_key(&pressed_keys, 0x5B));
        assert!(!pressed_keys_contain_virtual_key(&pressed_keys, 0x10));
    }
}
