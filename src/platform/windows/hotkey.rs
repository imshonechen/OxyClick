use std::collections::BTreeSet;

use crate::core::model::HotkeyBindings;
use crate::error::AppError;
use crate::platform::windows::hook::{HookMode, HookStatus, LowLevelKeyboardHook};
use crate::platform::windows::input::{
    is_bindable_virtual_key, is_hotkey_virtual_key_down, is_modifier_virtual_key,
    parse_virtual_key, pressed_keys_contain_virtual_key,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyRegistration {
    pub start_hotkey: String,
    pub stop_hotkey: String,
    pub panic_hotkey: Option<String>,
}

impl HotkeyRegistration {
    pub fn from_bindings(bindings: &HotkeyBindings) -> Self {
        Self {
            start_hotkey: bindings.start.clone(),
            stop_hotkey: bindings.stop.clone(),
            panic_hotkey: bindings.panic.clone(),
        }
    }

    pub fn validate(&self) -> Result<(), AppError> {
        ParsedHotkeys::parse(self).map(|_| ())
    }

    pub fn register(&self) -> Result<GlobalHotkeyManager, AppError> {
        GlobalHotkeyManager::new(ParsedHotkeys::parse(self)?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedHotkeys {
    start: HotkeyChord,
    stop: HotkeyChord,
    panic: Option<HotkeyChord>,
}

impl ParsedHotkeys {
    fn parse(registration: &HotkeyRegistration) -> Result<Self, AppError> {
        let start = HotkeyChord::parse("start", &registration.start_hotkey)?;
        let stop = HotkeyChord::parse("stop", &registration.stop_hotkey)?;
        let panic = registration
            .panic_hotkey
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|value| HotkeyChord::parse("panic", value))
            .transpose()?;

        if start.keys == stop.keys {
            return Err(AppError::HotkeyRegisterFailed(String::from(
                "开始和停止热键不能相同",
            )));
        }

        if let Some(panic) = &panic {
            if panic.keys == start.keys || panic.keys == stop.keys {
                return Err(AppError::HotkeyRegisterFailed(String::from(
                    "紧急停止热键必须和开始、停止热键不同",
                )));
            }
        }

        Ok(Self { start, stop, panic })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HotkeyChord {
    label: String,
    keys: Vec<u16>,
}

impl HotkeyChord {
    fn parse(kind: &str, raw: &str) -> Result<Self, AppError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(AppError::HotkeyRegisterFailed(format!(
                "{}热键不能为空",
                hotkey_kind_label(kind)
            )));
        }

        let mut keys = Vec::new();
        let mut non_modifier_key_count = 0_usize;
        for part in trimmed.split('+') {
            let token = part.trim();
            if token.is_empty() {
                return Err(AppError::HotkeyRegisterFailed(format!(
                    "{}热键里包含空的按键片段",
                    hotkey_kind_label(kind)
                )));
            }

            let virtual_key = parse_virtual_key(token).ok_or_else(|| {
                AppError::HotkeyRegisterFailed(format!(
                    "{}热键里包含不支持的按键：{token}",
                    hotkey_kind_label(kind)
                ))
            })?;

            if !is_bindable_virtual_key(virtual_key) {
                return Err(AppError::HotkeyRegisterFailed(format!(
                    "{}热键不支持设置该按键：{token}",
                    hotkey_kind_label(kind)
                )));
            }

            if !is_modifier_virtual_key(virtual_key) {
                non_modifier_key_count += 1;
            }

            if !keys.contains(&virtual_key) {
                keys.push(virtual_key);
            }
        }

        if keys.is_empty() {
            return Err(AppError::HotkeyRegisterFailed(format!(
                "{}热键至少要包含一个按键",
                hotkey_kind_label(kind)
            )));
        }

        if non_modifier_key_count == 0 {
            return Err(AppError::HotkeyRegisterFailed(format!(
                "{}热键不能只使用 Ctrl / Alt / Shift / Win，至少要有一个常规键",
                hotkey_kind_label(kind)
            )));
        }

        if non_modifier_key_count > 1 {
            return Err(AppError::HotkeyRegisterFailed(format!(
                "{}热键只能包含一个常规键，Ctrl / Alt / Shift / Win 可同时作为修饰键",
                hotkey_kind_label(kind)
            )));
        }

        keys.sort_unstable();

        Ok(Self {
            label: trimmed.to_owned(),
            keys,
        })
    }

    fn is_down_polling(&self) -> bool {
        self.keys.iter().all(|key| is_hotkey_virtual_key_down(*key))
    }

    fn is_down_in_pressed_keys(&self, pressed_keys: &BTreeSet<u16>) -> bool {
        self.keys
            .iter()
            .all(|key| pressed_keys_contain_virtual_key(pressed_keys, *key))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyPoll {
    pub start_pressed: bool,
    pub start_down: bool,
    pub stop_pressed: bool,
    pub stop_down: bool,
    pub panic_pressed: bool,
}

enum HotkeySource {
    StandardPolling,
    LowLevelHook {
        hook: LowLevelKeyboardHook,
        pressed_keys: BTreeSet<u16>,
    },
}

pub struct GlobalHotkeyManager {
    start: HotkeyChord,
    stop: HotkeyChord,
    panic: Option<HotkeyChord>,
    start_was_down: bool,
    stop_was_down: bool,
    panic_was_down: bool,
    source: HotkeySource,
    hook_status: HookStatus,
}

impl GlobalHotkeyManager {
    fn new(parsed: ParsedHotkeys) -> Result<Self, AppError> {
        let (source, hook_status) = match LowLevelKeyboardHook::install() {
            Ok(hook) => (
                HotkeySource::LowLevelHook {
                    hook,
                    pressed_keys: BTreeSet::new(),
                },
                HookStatus::install(HookMode::LowLevelKeyboard)?,
            ),
            Err(_) => (
                HotkeySource::StandardPolling,
                HookStatus::install(HookMode::StandardHotkey)?,
            ),
        };

        Ok(Self {
            start: parsed.start,
            stop: parsed.stop,
            panic: parsed.panic,
            start_was_down: false,
            stop_was_down: false,
            panic_was_down: false,
            source,
            hook_status,
        })
    }

    pub fn rebind(&mut self, registration: &HotkeyRegistration) -> Result<(), AppError> {
        let parsed = ParsedHotkeys::parse(registration)?;
        self.start = parsed.start;
        self.stop = parsed.stop;
        self.panic = parsed.panic;
        self.start_was_down = false;
        self.stop_was_down = false;
        self.panic_was_down = false;

        if let HotkeySource::LowLevelHook { pressed_keys, .. } = &mut self.source {
            pressed_keys.clear();
        }

        Ok(())
    }

    pub fn poll(&mut self) -> HotkeyPoll {
        let start = self.start.clone();
        let stop = self.stop.clone();
        let panic = self.panic.clone();

        let (start_down, stop_down, panic_down) = match &mut self.source {
            HotkeySource::StandardPolling => (
                start.is_down_polling(),
                stop.is_down_polling(),
                panic
                    .as_ref()
                    .map(HotkeyChord::is_down_polling)
                    .unwrap_or(false),
            ),
            HotkeySource::LowLevelHook { hook, pressed_keys } => match hook.drain_events() {
                Ok(events) => {
                    for event in events {
                        match event {
                            crate::platform::windows::hook::HookEvent::KeyDown(key) => {
                                pressed_keys.insert(key);
                            }
                            crate::platform::windows::hook::HookEvent::KeyUp(key) => {
                                pressed_keys.remove(&key);
                            }
                        }
                    }

                    (
                        start.is_down_in_pressed_keys(pressed_keys),
                        stop.is_down_in_pressed_keys(pressed_keys),
                        panic
                            .as_ref()
                            .map(|chord| chord.is_down_in_pressed_keys(pressed_keys))
                            .unwrap_or(false),
                    )
                }
                Err(_) => {
                    self.source = HotkeySource::StandardPolling;
                    self.hook_status = HookStatus::install(HookMode::StandardHotkey)
                        .expect("standard hotkey mode should be valid");

                    (
                        start.is_down_polling(),
                        stop.is_down_polling(),
                        panic
                            .as_ref()
                            .map(HotkeyChord::is_down_polling)
                            .unwrap_or(false),
                    )
                }
            },
        };

        let start_pressed = start_down && !self.start_was_down;
        let stop_pressed = stop_down && !self.stop_was_down;
        let panic_pressed = panic_down && !self.panic_was_down;

        self.start_was_down = start_down;
        self.stop_was_down = stop_down;
        self.panic_was_down = panic_down;

        HotkeyPoll {
            start_pressed,
            start_down,
            stop_pressed,
            stop_down,
            panic_pressed,
        }
    }

    pub fn summary(&self) -> String {
        match &self.panic {
            Some(panic) => format!(
                "开始 {} | 停止 {} | 紧急停止 {}",
                self.start.label, self.stop.label, panic.label
            ),
            None => format!("开始 {} | 停止 {}", self.start.label, self.stop.label),
        }
    }

    pub fn panic_label(&self) -> Option<&str> {
        self.panic.as_ref().map(|panic| panic.label.as_str())
    }

    pub fn backend_label(&self) -> &'static str {
        self.hook_status.label()
    }
}

fn hotkey_kind_label(kind: &str) -> &'static str {
    match kind {
        "start" => "开始",
        "stop" => "停止",
        "panic" => "紧急停止",
        _ => "热键",
    }
}

#[cfg(test)]
mod tests {
    use super::HotkeyRegistration;

    #[test]
    fn accepts_default_hotkeys() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("F6"),
            stop_hotkey: String::from("F7"),
            panic_hotkey: Some(String::from("Ctrl+Alt+Pause")),
        };

        assert!(registration.validate().is_ok());
        assert!(registration.register().is_ok());
    }

    #[test]
    fn rejects_duplicate_start_stop_hotkeys() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("F6"),
            stop_hotkey: String::from("f6"),
            panic_hotkey: None,
        };

        assert!(registration.register().is_err());
    }

    #[test]
    fn rejects_invalid_key_names() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("MagicButton"),
            stop_hotkey: String::from("F7"),
            panic_hotkey: None,
        };

        assert!(registration.register().is_err());
    }

    #[test]
    fn accepts_symbol_hotkeys() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("Ctrl+/"),
            stop_hotkey: String::from("Shift+["),
            panic_hotkey: Some(String::from("NumAdd")),
        };

        assert!(registration.validate().is_ok());
    }

    #[test]
    fn rejects_tab_and_esc_hotkeys() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("Tab"),
            stop_hotkey: String::from("F7"),
            panic_hotkey: None,
        };

        assert!(registration.validate().is_err());

        let registration = HotkeyRegistration {
            start_hotkey: String::from("Esc"),
            stop_hotkey: String::from("F7"),
            panic_hotkey: None,
        };

        assert!(registration.validate().is_err());
    }

    #[test]
    fn rejects_multiple_regular_keys_in_hotkeys() {
        let registration = HotkeyRegistration {
            start_hotkey: String::from("Ctrl+A+1"),
            stop_hotkey: String::from("F7"),
            panic_hotkey: None,
        };

        assert!(registration.validate().is_err());
    }
}
