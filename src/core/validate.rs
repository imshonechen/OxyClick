use crate::core::model::{ClickTaskConfig, RunMode};
use crate::error::AppError;

pub const MIN_INTERVAL_MS: u64 = 1;
pub const MIN_DURATION_MS: u64 = 1;

pub fn validate_config(config: &ClickTaskConfig) -> Result<(), AppError> {
    if config.name.trim().is_empty() {
        return Err(AppError::InvalidConfig(String::from("配置名称不能为空")));
    }

    if config.interval_ms < MIN_INTERVAL_MS {
        return Err(AppError::InvalidConfig(format!(
            "点击间隔不能小于 {MIN_INTERVAL_MS} ms",
        )));
    }

    if config.press_duration_ms > config.interval_ms {
        return Err(AppError::InvalidConfig(String::from(
            "按下时长不能大于点击间隔",
        )));
    }

    if config.hotkeys.start == config.hotkeys.stop {
        return Err(AppError::InvalidConfig(String::from(
            "开始热键和停止热键不能相同",
        )));
    }

    match config.run_mode {
        RunMode::Infinite => {}
        RunMode::Count { total } if total == 0 => {
            return Err(AppError::InvalidConfig(String::from(
                "计数模式的执行次数必须大于 0",
            )));
        }
        RunMode::Count { .. } => {}
        RunMode::Timed { duration_ms } if duration_ms < MIN_DURATION_MS => {
            return Err(AppError::InvalidConfig(String::from(
                "限时模式的运行时长必须大于 0",
            )));
        }
        RunMode::Timed { .. } => {}
    }

    if let Some(jitter_ms) = config.jitter_ms {
        if jitter_ms > config.interval_ms {
            return Err(AppError::InvalidConfig(String::from(
                "抖动范围不能大于点击间隔",
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_config;
    use crate::core::model::{ClickTaskConfig, RunMode};

    #[test]
    fn accepts_default_config() {
        let config = ClickTaskConfig::default();
        assert!(validate_config(&config).is_ok());
    }

    #[test]
    fn rejects_zero_count() {
        let mut config = ClickTaskConfig::default();
        config.run_mode = RunMode::Count { total: 0 };
        assert!(validate_config(&config).is_err());
    }

    #[test]
    fn rejects_identical_hotkeys() {
        let mut config = ClickTaskConfig::default();
        config.hotkeys.stop = config.hotkeys.start.clone();
        assert!(validate_config(&config).is_err());
    }
}
