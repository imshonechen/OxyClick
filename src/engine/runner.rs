use std::marker::PhantomData;
use std::sync::{
    mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::core::model::ClickTaskConfig;
use crate::core::state::EngineState;
use crate::core::validate::validate_config;
use crate::engine::scheduler::{next_interval_ms, should_stop};
use crate::error::AppError;
use crate::platform::windows::input::{validate_bindable_input_action, InputBackend};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    Completed { completed_actions: u64 },
    Failed {
        message: String,
        completed_actions: u64,
    },
}

#[derive(Debug, Clone)]
struct EngineSnapshot {
    state: EngineState,
    config: Option<ClickTaskConfig>,
    completed_actions: u64,
}

impl Default for EngineSnapshot {
    fn default() -> Self {
        Self {
            state: EngineState::Idle,
            config: None,
            completed_actions: 0,
        }
    }
}

enum WorkerCommand {
    Arm(ClickTaskConfig, Sender<Result<(), AppError>>),
    Start(Sender<Result<(), AppError>>),
    Stop(Sender<()>),
    Shutdown,
}

pub struct EngineRunner<B>
where
    B: InputBackend + Send + 'static,
{
    command_tx: Sender<WorkerCommand>,
    event_rx: Receiver<EngineEvent>,
    snapshot: Arc<Mutex<EngineSnapshot>>,
    worker_handle: Option<JoinHandle<()>>,
    _backend: PhantomData<B>,
}

impl<B> EngineRunner<B>
where
    B: InputBackend + Send + 'static,
{
    pub fn new(backend: B) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();
        let snapshot = Arc::new(Mutex::new(EngineSnapshot::default()));
        let worker_snapshot = Arc::clone(&snapshot);
        let worker_handle = thread::spawn(move || {
            run_worker(backend, command_rx, event_tx, worker_snapshot);
        });

        Self {
            command_tx,
            event_rx,
            snapshot,
            worker_handle: Some(worker_handle),
            _backend: PhantomData,
        }
    }

    pub fn state(&self) -> EngineState {
        self.read_snapshot(|snapshot| snapshot.state.clone())
    }

    pub fn arm(&mut self, config: ClickTaskConfig) -> Result<(), AppError> {
        validate_config(&config)?;
        validate_bindable_input_action(&config.action)?;

        let (ack_tx, ack_rx) = mpsc::channel();
        self.command_tx
            .send(WorkerCommand::Arm(config, ack_tx))
            .map_err(|_| Self::command_send_error())?;

        ack_rx.recv().map_err(|_| Self::command_send_error())?
    }

    pub fn start(&mut self) -> Result<(), AppError> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.command_tx
            .send(WorkerCommand::Start(ack_tx))
            .map_err(|_| Self::command_send_error())?;

        ack_rx.recv().map_err(|_| Self::command_send_error())?
    }

    pub fn stop(&mut self) -> Result<(), AppError> {
        let (ack_tx, ack_rx) = mpsc::channel();
        self.command_tx
            .send(WorkerCommand::Stop(ack_tx))
            .map_err(|_| Self::command_send_error())?;

        ack_rx.recv().map_err(|_| Self::command_send_error())
    }

    pub fn completed_actions(&self) -> u64 {
        self.read_snapshot(|snapshot| snapshot.completed_actions)
    }

    pub fn config(&self) -> Option<ClickTaskConfig> {
        self.read_snapshot(|snapshot| snapshot.config.clone())
    }

    pub fn poll_event(&self) -> Option<EngineEvent> {
        match self.event_rx.try_recv() {
            Ok(event) => Some(event),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => None,
        }
    }

    fn read_snapshot<T>(&self, map: impl FnOnce(&EngineSnapshot) -> T) -> T {
        let snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map(&snapshot)
    }

    fn command_send_error() -> AppError {
        AppError::Io(String::from("执行线程通信失败"))
    }
}

impl<B> Drop for EngineRunner<B>
where
    B: InputBackend + Send + 'static,
{
    fn drop(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Shutdown);
        if let Some(worker_handle) = self.worker_handle.take() {
            let _ = worker_handle.join();
        }
    }
}

fn run_worker<B>(
    mut backend: B,
    command_rx: Receiver<WorkerCommand>,
    event_tx: Sender<EngineEvent>,
    snapshot: Arc<Mutex<EngineSnapshot>>,
) where
    B: InputBackend,
{
    let mut state = EngineState::Idle;
    let mut config: Option<ClickTaskConfig> = None;
    let mut completed_actions = 0_u64;
    let mut started_at: Option<Instant> = None;
    let mut next_tick_at: Option<Instant> = None;

    loop {
        if matches!(state, EngineState::Running) {
            let Some(next_tick) = next_tick_at else {
                if let Some(active_config) = config.as_ref() {
                    next_tick_at = Some(Instant::now() + next_interval(active_config, completed_actions));
                    continue;
                }

                state = EngineState::Idle;
                started_at = None;
                write_snapshot(&snapshot, &state, config.as_ref(), completed_actions);
                continue;
            };

            match command_rx.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
                Ok(command) => {
                    if handle_command(
                        command,
                        &mut state,
                        &mut config,
                        &mut completed_actions,
                        &mut started_at,
                        &mut next_tick_at,
                        &snapshot,
                    ) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Timeout) => {
                    let Some(active_config) = config.as_ref() else {
                        state = EngineState::Idle;
                        started_at = None;
                        next_tick_at = None;
                        write_snapshot(&snapshot, &state, config.as_ref(), completed_actions);
                        continue;
                    };

                    match backend.send_action(&active_config.action, active_config.press_duration_ms) {
                        Ok(()) => {
                            completed_actions += 1;
                            write_snapshot(&snapshot, &state, config.as_ref(), completed_actions);

                            let elapsed = started_at
                                .map(|started_at| started_at.elapsed())
                                .unwrap_or(Duration::ZERO);

                            if should_stop(&active_config.run_mode, completed_actions, elapsed) {
                                state = EngineState::Idle;
                                started_at = None;
                                next_tick_at = None;
                                write_snapshot(&snapshot, &state, config.as_ref(), completed_actions);
                                let _ = event_tx.send(EngineEvent::Completed { completed_actions });
                            } else {
                                next_tick_at = Some(
                                    Instant::now()
                                        + next_interval(active_config, completed_actions),
                                );
                            }
                        }
                        Err(error) => {
                            let message = error.to_string();
                            state = EngineState::Error(message.clone());
                            started_at = None;
                            next_tick_at = None;
                            write_snapshot(&snapshot, &state, config.as_ref(), completed_actions);
                            let _ = event_tx.send(EngineEvent::Failed {
                                message,
                                completed_actions,
                            });
                        }
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match command_rx.recv() {
                Ok(command) => {
                    if handle_command(
                        command,
                        &mut state,
                        &mut config,
                        &mut completed_actions,
                        &mut started_at,
                        &mut next_tick_at,
                        &snapshot,
                    ) {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    }
}

fn handle_command(
    command: WorkerCommand,
    state: &mut EngineState,
    config: &mut Option<ClickTaskConfig>,
    completed_actions: &mut u64,
    started_at: &mut Option<Instant>,
    next_tick_at: &mut Option<Instant>,
    snapshot: &Arc<Mutex<EngineSnapshot>>,
) -> bool {
    match command {
        WorkerCommand::Arm(next_config, ack_tx) => {
            if matches!(state, EngineState::Running | EngineState::Stopping) {
                let _ = ack_tx.send(Err(AppError::InvalidConfig(String::from(
                    "引擎运行中，无法重新装载配置",
                ))));
                return false;
            }

            *config = Some(next_config);
            *completed_actions = 0;
            *started_at = None;
            *next_tick_at = None;
            *state = EngineState::Armed;
            write_snapshot(snapshot, state, config.as_ref(), *completed_actions);
            let _ = ack_tx.send(Ok(()));
            false
        }
        WorkerCommand::Start(ack_tx) => {
            let Some(active_config) = config.as_ref() else {
                let _ = ack_tx.send(Err(AppError::InvalidConfig(String::from(
                    "没有可用配置，无法启动引擎",
                ))));
                return false;
            };

            if matches!(state, EngineState::Running) {
                let _ = ack_tx.send(Err(AppError::InvalidConfig(String::from(
                    "引擎已经在运行中",
                ))));
                return false;
            }

            *completed_actions = 0;
            let now = Instant::now();
            *started_at = Some(now);
            *next_tick_at = Some(now + next_interval(active_config, *completed_actions));
            *state = EngineState::Running;
            write_snapshot(snapshot, state, config.as_ref(), *completed_actions);
            let _ = ack_tx.send(Ok(()));
            false
        }
        WorkerCommand::Stop(ack_tx) => {
            *started_at = None;
            *next_tick_at = None;
            *state = EngineState::Idle;
            write_snapshot(snapshot, state, config.as_ref(), *completed_actions);
            let _ = ack_tx.send(());
            false
        }
        WorkerCommand::Shutdown => true,
    }
}

fn write_snapshot(
    snapshot: &Arc<Mutex<EngineSnapshot>>,
    state: &EngineState,
    config: Option<&ClickTaskConfig>,
    completed_actions: u64,
) {
    let mut snapshot = snapshot
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    snapshot.state = state.clone();
    snapshot.config = config.cloned();
    snapshot.completed_actions = completed_actions;
}

fn next_interval(config: &ClickTaskConfig, iteration: u64) -> Duration {
    let interval_ms = next_interval_ms(config.interval_ms.max(1), config.jitter_ms, iteration).max(1);
    Duration::from_millis(interval_ms)
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use super::{EngineEvent, EngineRunner};
    use crate::core::model::{ClickTaskConfig, RunMode};
    use crate::core::state::EngineState;
    use crate::platform::windows::input::NoopInputBackend;

    fn wait_for(
        mut condition: impl FnMut() -> bool,
        timeout: Duration,
        label: &str,
    ) {
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            if condition() {
                return;
            }
            thread::sleep(Duration::from_millis(5));
        }

        panic!("timed out while waiting for {label}");
    }

    #[test]
    fn engine_moves_from_armed_to_running_to_idle() {
        let config = ClickTaskConfig::default();
        let mut runner = EngineRunner::new(NoopInputBackend::default());

        runner.arm(config).expect("arm should succeed");
        assert_eq!(runner.state(), EngineState::Armed);

        runner.start().expect("start should succeed");
        assert_eq!(runner.state(), EngineState::Running);

        runner.stop().expect("stop should succeed");
        assert_eq!(runner.state(), EngineState::Idle);
    }

    #[test]
    fn count_mode_stops_after_target() {
        let mut config = ClickTaskConfig::default();
        config.interval_ms = 1;
        config.press_duration_ms = 0;
        config.jitter_ms = Some(0);
        config.run_mode = RunMode::Count { total: 1 };

        let mut runner = EngineRunner::new(NoopInputBackend::default());
        runner.arm(config).expect("arm should succeed");
        runner.start().expect("start should succeed");

        wait_for(
            || matches!(runner.poll_event(), Some(EngineEvent::Completed { .. })),
            Duration::from_secs(1),
            "count mode completion",
        );

        assert_eq!(runner.completed_actions(), 1);
        assert_eq!(runner.state(), EngineState::Idle);
    }
}
