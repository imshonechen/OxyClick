use std::time::{Duration, Instant};

use crate::core::model::ClickTaskConfig;
use crate::core::state::EngineState;
use crate::core::validate::validate_config;
use crate::engine::scheduler::should_stop;
use crate::error::AppError;
use crate::platform::windows::input::{validate_bindable_input_action, InputBackend};

pub struct EngineRunner<B>
where
    B: InputBackend,
{
    state: EngineState,
    backend: B,
    config: Option<ClickTaskConfig>,
    started_at: Option<Instant>,
    completed_actions: u64,
}

impl<B> EngineRunner<B>
where
    B: InputBackend,
{
    pub fn new(backend: B) -> Self {
        Self {
            state: EngineState::Idle,
            backend,
            config: None,
            started_at: None,
            completed_actions: 0,
        }
    }

    pub fn state(&self) -> &EngineState {
        &self.state
    }

    pub fn arm(&mut self, config: ClickTaskConfig) -> Result<(), AppError> {
        validate_config(&config)?;
        validate_bindable_input_action(&config.action)?;
        self.config = Some(config);
        self.completed_actions = 0;
        self.started_at = None;
        self.state = EngineState::Armed;
        Ok(())
    }

    pub fn start(&mut self) -> Result<(), AppError> {
        if self.config.is_none() {
            return Err(AppError::InvalidConfig(String::from(
                "没有可用配置，无法启动引擎",
            )));
        }

        self.started_at = Some(Instant::now());
        self.completed_actions = 0;
        self.state = EngineState::Running;
        Ok(())
    }

    pub fn stop(&mut self) {
        self.state = EngineState::Stopping;
        self.started_at = None;
        self.state = EngineState::Idle;
    }

    pub fn tick(&mut self) -> Result<bool, AppError> {
        if self.state != EngineState::Running {
            return Ok(false);
        }

        let config = self
            .config
            .as_ref()
            .ok_or_else(|| AppError::InvalidConfig(String::from("缺少当前活动配置")))?;

        self.backend
            .send_action(&config.action, config.press_duration_ms)?;
        self.completed_actions += 1;

        let elapsed = self
            .started_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or(Duration::ZERO);

        if should_stop(&config.run_mode, self.completed_actions, elapsed) {
            self.stop();
        }

        Ok(true)
    }

    pub fn completed_actions(&self) -> u64 {
        self.completed_actions
    }

    pub fn config(&self) -> Option<&ClickTaskConfig> {
        self.config.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::EngineRunner;
    use crate::core::model::{ClickTaskConfig, RunMode};
    use crate::core::state::EngineState;
    use crate::platform::windows::input::NoopInputBackend;

    #[test]
    fn engine_moves_from_armed_to_running_to_idle() {
        let config = ClickTaskConfig::default();
        let mut runner = EngineRunner::new(NoopInputBackend::default());

        runner.arm(config).expect("arm should succeed");
        assert_eq!(runner.state(), &EngineState::Armed);

        runner.start().expect("start should succeed");
        assert_eq!(runner.state(), &EngineState::Running);

        runner.stop();
        assert_eq!(runner.state(), &EngineState::Idle);
    }

    #[test]
    fn count_mode_stops_after_target() {
        let mut config = ClickTaskConfig::default();
        config.run_mode = RunMode::Count { total: 1 };

        let mut runner = EngineRunner::new(NoopInputBackend::default());
        runner.arm(config).expect("arm should succeed");
        runner.start().expect("start should succeed");

        let did_send = runner.tick().expect("tick should succeed");
        assert!(did_send);
        assert_eq!(runner.completed_actions(), 1);
        assert_eq!(runner.state(), &EngineState::Idle);
    }
}
