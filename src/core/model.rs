use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TriggerMode {
    Hold,
    #[default]
    Toggle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RunMode {
    Infinite,
    Count { total: u64 },
    Timed { duration_ms: u64 },
}

impl Default for RunMode {
    fn default() -> Self {
        Self::Infinite
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputAction {
    MouseClick {
        button: MouseButton,
    },
    KeyPress {
        key_code: String,
    },
    KeyCombo {
        modifiers: Vec<String>,
        key_code: String,
    },
}

impl Default for InputAction {
    fn default() -> Self {
        Self::MouseClick {
            button: MouseButton::Left,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HotkeyBindings {
    pub start: String,
    pub stop: String,
    pub panic: Option<String>,
}

impl Default for HotkeyBindings {
    fn default() -> Self {
        Self {
            start: String::from("F6"),
            stop: String::from("F7"),
            panic: Some(String::from("Ctrl+Alt+Pause")),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClickTaskConfig {
    pub name: String,
    pub trigger_mode: TriggerMode,
    pub run_mode: RunMode,
    pub action: InputAction,
    pub start_delay_ms: u64,
    pub interval_ms: u64,
    pub press_duration_ms: u64,
    pub hotkeys: HotkeyBindings,
    pub jitter_ms: Option<u64>,
    pub stop_on_focus_lost: bool,
}

impl Default for ClickTaskConfig {
    fn default() -> Self {
        Self {
            name: String::from("默认配置"),
            trigger_mode: TriggerMode::Toggle,
            run_mode: RunMode::Infinite,
            action: InputAction::MouseClick {
                button: MouseButton::Left,
            },
            start_delay_ms: 700,
            interval_ms: 25,
            press_duration_ms: 5,
            hotkeys: HotkeyBindings::default(),
            jitter_ms: Some(0),
            stop_on_focus_lost: true,
        }
    }
}
