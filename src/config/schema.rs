use serde::{Deserialize, Serialize};

use crate::core::model::ClickTaskConfig;
use crate::core::validate::validate_config;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub stop_on_focus_lost: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            stop_on_focus_lost: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub general: GeneralConfig,
    pub profiles: Vec<ClickTaskConfig>,
    pub active_profile_index: usize,
}

impl AppConfig {
    pub fn active_profile(&self) -> &ClickTaskConfig {
        self.profiles
            .get(self.active_profile_index)
            .unwrap_or(&self.profiles[0])
    }

    pub fn active_profile_mut(&mut self) -> &mut ClickTaskConfig {
        self.normalize();

        let index = self
            .active_profile_index
            .min(self.profiles.len().saturating_sub(1));
        self.active_profile_index = index;
        &mut self.profiles[index]
    }

    pub fn normalize(&mut self) {
        if self.profiles.is_empty() {
            self.profiles.push(ClickTaskConfig::default());
            self.active_profile_index = 0;
        }

        for profile in &mut self.profiles {
            if profile.name == "Default Profile" {
                profile.name = String::from("默认配置");
            }

            if matches!(profile.hotkeys.panic.as_deref(), Some("控制+Alt+暂停")) {
                profile.hotkeys.panic = Some(String::from("Ctrl+Alt+Pause"));
            }
        }

        if self.active_profile_index >= self.profiles.len() {
            self.active_profile_index = 0;
        }

        self.general.stop_on_focus_lost = self.active_profile().stop_on_focus_lost;
    }

    pub fn validate(&self) -> Result<(), AppError> {
        if self.profiles.is_empty() {
            return Err(AppError::InvalidConfig(String::from(
                "配置文件里至少要包含一个配置项",
            )));
        }

        for profile in &self.profiles {
            validate_config(profile)?;
        }

        Ok(())
    }

    pub fn to_toml_string(&self) -> Result<String, AppError> {
        toml::to_string_pretty(self)
            .map_err(|error| AppError::InvalidConfig(format!("配置序列化失败：{error}")))
    }

    pub fn from_toml_str(contents: &str) -> Result<Self, AppError> {
        let mut config: Self = toml::from_str(contents)
            .map_err(|error| AppError::InvalidConfig(format!("解析 config.toml 失败：{error}")))?;
        config.normalize();
        config.validate()?;
        Ok(config)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            profiles: vec![ClickTaskConfig::default()],
            active_profile_index: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AppConfig;
    use crate::core::model::{InputAction, MouseButton, RunMode, TriggerMode};

    #[test]
    fn round_trips_toml() {
        let mut config = AppConfig::default();
        let profile = config.active_profile_mut();
        profile.name = String::from("Rapid Fire");
        profile.trigger_mode = TriggerMode::Hold;
        profile.run_mode = RunMode::Count { total: 321 };
        profile.action = InputAction::MouseClick {
            button: MouseButton::Right,
        };
        profile.start_delay_ms = 900;
        profile.hotkeys.start = String::from("F8");
        profile.hotkeys.stop = String::from("F9");

        let toml = config.to_toml_string().expect("config should serialize");
        let parsed = AppConfig::from_toml_str(&toml).expect("config should deserialize");

        assert_eq!(parsed, config);
    }

    #[test]
    fn migrates_legacy_default_profile_labels() {
        let contents = r#"
active_profile_index = 0

[general]
launch_on_startup = false
stop_on_focus_lost = true

[[profiles]]
name = "Default Profile"
trigger_mode = "toggle"
interval_ms = 25
press_duration_ms = 5
jitter_ms = 0
stop_on_focus_lost = true

[profiles.run_mode]
kind = "infinite"

[profiles.action]
kind = "mouse_click"
button = "left"

[profiles.hotkeys]
start = "F6"
stop = "F7"
panic = "Ctrl+Alt+Pause"
"#;

        let config = AppConfig::from_toml_str(contents).expect("旧配置应能被迁移");
        let profile = config.active_profile();

        assert_eq!(profile.name, "默认配置");
        assert_eq!(profile.start_delay_ms, 700);
        assert_eq!(profile.hotkeys.panic.as_deref(), Some("Ctrl+Alt+Pause"));
    }
}
