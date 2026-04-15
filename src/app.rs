use std::error::Error;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use crate::config::file::{
    default_config_path, load_or_default, portable_config_path, write_config,
};
use crate::config::schema::AppConfig;
use crate::core::model::{ClickTaskConfig, InputAction, MouseButton, RunMode, TriggerMode};
use crate::core::state::EngineState;
use crate::core::validate::validate_config;
use crate::engine::runner::EngineRunner;
use crate::error::AppError;
use crate::platform::windows::focus::current_foreground_window;
use crate::platform::windows::hotkey::{GlobalHotkeyManager, HotkeyRegistration};
use crate::platform::windows::input::{
    poll_hotkey_capture, validate_bindable_input_action, BackendMode, WindowsInputBackend,
};
use crate::ui::panels::{render_status_panel, StatusPanelData};
use crate::ui::theme;
use crate::ui::widgets::{
    card_with_body_spacing_and_min_height_and_metrics, form_grid, form_note_row, form_row,
    hotkey_capture_field, number_field, number_row,
};

const DEFAULT_WINDOW_WIDTH: f32 = 1120.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 680.0;
const MIN_WINDOW_WIDTH: f32 = 960.0;
const MIN_WINDOW_HEIGHT: f32 = 640.0;
const SAVE_BUTTON_FEEDBACK_DURATION: Duration = Duration::from_secs(1);

pub struct Application {
    config: AppConfig,
    engine: EngineRunner<WindowsInputBackend>,
    hotkeys: GlobalHotkeyManager,
    backend_mode: BackendMode,
    config_path: PathBuf,
    live_config_error: Option<String>,
    notification: Option<UiNotification>,
    save_button_feedback_started_at: Option<Instant>,
    delayed_start_at: Option<Instant>,
    pending_action: Option<PendingUiAction>,
    last_tick_at: Option<Instant>,
    started_running_at: Option<Instant>,
    last_run_duration: Duration,
    last_completed_actions: u64,
    cards_display_height: f32,
    profile_editor_actual_height: f32,
    hotkeys_actual_height: f32,
    capture_state: Option<ShortcutCaptureState>,
    focus_stop_guard: Option<FocusStopGuard>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationKind {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingUiAction {
    Start,
    Stop,
    CancelStart,
    Save,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartOrigin {
    UiButton,
    Hotkey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureTarget {
    Action,
    Hotkey(HotkeyField),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HotkeyField {
    Start,
    Stop,
    Panic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UiNotification {
    message: String,
    kind: NotificationKind,
    expires_at: Instant,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShortcutCaptureState {
    target: CaptureTarget,
    preview_label: Option<String>,
    last_valid_label: Option<String>,
    saw_pressed_keys: bool,
    paused_by_focus_loss: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct FocusStopGuard {
    target_window_handle: Option<isize>,
}

#[derive(Debug, Clone, Copy)]
struct LayoutDebugMetrics {
    profile_editor_actual_height: f32,
    hotkeys_actual_height: f32,
    cards_display_height: f32,
    profile_editor_fill_height: f32,
    hotkeys_fill_height: f32,
}

impl UiNotification {
    fn new(kind: NotificationKind, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind,
            expires_at: Instant::now() + Duration::from_secs(3),
        }
    }
}

impl Application {
    pub fn bootstrap() -> Result<Self, AppError> {
        let primary_config_path = default_config_path();
        let (config_path, mut config) = match load_or_default(&primary_config_path) {
            Ok(config) => (primary_config_path, config),
            Err(AppError::Io(_)) => {
                let fallback_path = portable_config_path();
                let config = load_or_default(&fallback_path)?;
                (fallback_path, config)
            }
            Err(error) => return Err(error),
        };
        let config_before_normalize = config.clone();
        config.normalize();
        if config != config_before_normalize {
            let _ = write_config(&config_path, &config);
        }
        let active_profile = config.active_profile().clone();

        validate_config(&active_profile)?;

        let backend = WindowsInputBackend::detect();
        let backend_mode = backend.mode();
        let hotkeys = HotkeyRegistration::from_bindings(&active_profile.hotkeys).register()?;
        let mut engine = EngineRunner::new(backend);
        engine.arm(active_profile)?;

        Ok(Self {
            config,
            engine,
            hotkeys,
            backend_mode,
            config_path,
            live_config_error: None,
            notification: Some(UiNotification::new(
                NotificationKind::Info,
                "配置已加载，表单修改会自动用于下一次启动。",
            )),
            save_button_feedback_started_at: None,
            delayed_start_at: None,
            pending_action: None,
            last_tick_at: None,
            started_running_at: None,
            last_run_duration: Duration::ZERO,
            last_completed_actions: 0,
            cards_display_height: 0.0,
            profile_editor_actual_height: 0.0,
            hotkeys_actual_height: 0.0,
            capture_state: None,
            focus_stop_guard: None,
        })
    }

    pub fn summary(&self) -> String {
        let profile = self.runtime_profile();

        format!(
            "OxyClick 已启动。\n当前配置：{}\n引擎状态：{}\n输入后端：{}\n热键配置：{}\n热键后端：{}\n下一步：继续增加预设管理和更丰富的诊断信息。",
            profile.name,
            self.engine.state(),
            self.backend_mode.label(),
            self.hotkeys.summary(),
            self.hotkeys.backend_label(),
        )
    }

    fn is_running(&self) -> bool {
        self.engine.state() == &EngineState::Running
    }

    fn has_pending_start(&self) -> bool {
        self.delayed_start_at.is_some()
    }

    fn runtime_profile(&self) -> &ClickTaskConfig {
        self.engine
            .config()
            .unwrap_or_else(|| self.config.active_profile())
    }

    fn pending_start_remaining(&self) -> Option<Duration> {
        self.delayed_start_at
            .map(|start_at| start_at.saturating_duration_since(Instant::now()))
    }

    fn pending_start_countdown_text(&self) -> Option<String> {
        self.pending_start_remaining()
            .map(Self::format_countdown_duration)
    }

    fn format_countdown_duration(duration: Duration) -> String {
        let remaining_ms = duration.as_millis().max(1) as u64;
        let rounded_ms = remaining_ms.div_ceil(100) * 100;

        if rounded_ms >= 1_000 {
            format!("{:.1} 秒", rounded_ms as f32 / 1_000.0)
        } else {
            format!("{rounded_ms} ms")
        }
    }

    fn push_notification(&mut self, kind: NotificationKind, message: impl Into<String>) {
        self.notification = Some(UiNotification::new(kind, message));
    }

    fn expire_notification(&mut self) {
        if self
            .notification
            .as_ref()
            .is_some_and(|notice| notice.expires_at <= Instant::now())
        {
            self.notification = None;
        }
    }

    fn trigger_save_button_feedback(&mut self) {
        self.save_button_feedback_started_at = Some(Instant::now());
    }

    fn expire_save_button_feedback(&mut self) {
        if self
            .save_button_feedback_started_at
            .is_some_and(|started_at| started_at.elapsed() >= SAVE_BUTTON_FEEDBACK_DURATION)
        {
            self.save_button_feedback_started_at = None;
        }
    }

    fn save_button_feedback_progress(&self) -> Option<f32> {
        self.save_button_feedback_started_at.map(|started_at| {
            (started_at.elapsed().as_secs_f32() / SAVE_BUTTON_FEEDBACK_DURATION.as_secs_f32())
                .clamp(0.0, 1.0)
        })
    }

    fn lerp_color(from: Color32, to: Color32, progress: f32) -> Color32 {
        let progress = progress.clamp(0.0, 1.0);
        let [from_r, from_g, from_b, from_a] = from.to_array();
        let [to_r, to_g, to_b, to_a] = to.to_array();
        let lerp = |start: u8, end: u8| -> u8 {
            (start as f32 + (end as f32 - start as f32) * progress).round() as u8
        };

        Color32::from_rgba_unmultiplied(
            lerp(from_r, to_r),
            lerp(from_g, to_g),
            lerp(from_b, to_b),
            lerp(from_a, to_a),
        )
    }

    fn snapshot_last_run_metrics(&mut self) {
        self.last_completed_actions = self.engine.completed_actions();
        self.last_run_duration = self
            .started_running_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or(Duration::ZERO);
    }

    fn displayed_completed_actions(&self) -> u64 {
        if self.is_running() {
            self.engine.completed_actions()
        } else {
            self.last_completed_actions
        }
    }

    fn displayed_run_duration(&self) -> Duration {
        self.started_running_at
            .map(|started_at| started_at.elapsed())
            .unwrap_or(self.last_run_duration)
    }

    fn format_elapsed_duration(duration: Duration) -> String {
        let total_seconds = duration.as_secs();

        if total_seconds >= 3600 {
            format!(
                "{} 小时 {:02} 分",
                total_seconds / 3600,
                (total_seconds % 3600) / 60
            )
        } else if total_seconds >= 60 {
            format!("{} 分 {:02} 秒", total_seconds / 60, total_seconds % 60)
        } else {
            format!("{:.1} 秒", duration.as_secs_f32())
        }
    }

    fn sync_runtime_config(&mut self) -> Result<(), AppError> {
        self.sync_config_state();
        let profile = self.config.active_profile().clone();
        let hotkey_registration = HotkeyRegistration::from_bindings(&profile.hotkeys);

        self.validate_active_profile_for_persistence(&hotkey_registration)?;
        self.hotkeys.rebind(&hotkey_registration)?;

        if !self.is_running() {
            self.engine.arm(profile)?;
            self.last_tick_at = None;
        }

        Ok(())
    }

    fn reconcile_live_config(&mut self) {
        match self.sync_runtime_config() {
            Ok(()) => {
                self.live_config_error = None;
            }
            Err(error) => {
                self.live_config_error = Some(error.to_string());
            }
        }
    }

    fn arm_focus_stop_guard(&mut self) {
        let target_window_handle = if self.runtime_profile().stop_on_focus_lost {
            current_foreground_window()
                .filter(|window| !window.belongs_to_current_process())
                .map(|window| window.handle)
        } else {
            None
        };

        self.focus_stop_guard =
            self.runtime_profile()
                .stop_on_focus_lost
                .then_some(FocusStopGuard {
                    target_window_handle,
                });
    }

    fn clear_focus_stop_guard(&mut self) {
        self.focus_stop_guard = None;
    }

    fn start_engine_now(&mut self) {
        self.delayed_start_at = None;
        match self.sync_runtime_config() {
            Ok(()) => {
                self.live_config_error = None;
                match self.engine.start() {
                    Ok(()) => {
                        let started_at = Instant::now();
                        self.last_tick_at = Some(started_at);
                        self.started_running_at = Some(started_at);
                        self.last_run_duration = Duration::ZERO;
                        self.last_completed_actions = 0;
                        self.arm_focus_stop_guard();
                    }
                    Err(error) => {
                        self.clear_focus_stop_guard();
                        self.push_notification(
                            NotificationKind::Error,
                            format!("无法开始运行：{error}"),
                        );
                    }
                }
            }
            Err(error) => {
                self.clear_focus_stop_guard();
                self.live_config_error = Some(error.to_string());
                self.push_notification(NotificationKind::Error, format!("无法开始运行：{error}"));
            }
        }
    }

    fn start_engine(&mut self, origin: StartOrigin) {
        let profile = self.config.active_profile();
        let should_delay = matches!(origin, StartOrigin::UiButton) && profile.start_delay_ms > 0;

        if should_delay {
            let delay = Duration::from_millis(profile.start_delay_ms);
            self.delayed_start_at = Some(Instant::now() + delay);
            self.push_notification(
                NotificationKind::Info,
                format!(
                    "将在 {} ms 后开始运行，请先切回目标窗口或移开鼠标。",
                    profile.start_delay_ms
                ),
            );
            return;
        }

        self.start_engine_now();
    }

    fn stop_engine(&mut self) {
        self.snapshot_last_run_metrics();
        self.engine.stop();
        self.delayed_start_at = None;
        self.last_tick_at = None;
        self.started_running_at = None;
        self.clear_focus_stop_guard();
        self.reconcile_live_config();
    }

    fn cancel_delayed_start(&mut self) {
        if self.delayed_start_at.take().is_some() {
            self.push_notification(NotificationKind::Info, "已取消启动。");
        }
    }

    fn save_config(&mut self) {
        self.sync_config_state();
        let hotkey_registration =
            HotkeyRegistration::from_bindings(&self.config.active_profile().hotkeys);

        match self.validate_active_profile_for_persistence(&hotkey_registration) {
            Ok(()) => {
                self.live_config_error = None;
                match self.persist_config() {
                    Ok(()) => {
                        self.trigger_save_button_feedback();
                        self.push_notification(
                            NotificationKind::Success,
                            format!("配置已保存到 {}", self.config_path.display()),
                        );
                    }
                    Err(error) => {
                        self.save_button_feedback_started_at = None;
                        self.push_notification(
                            NotificationKind::Error,
                            format!("保存失败：{error}"),
                        );
                    }
                }
            }
            Err(error) => {
                self.save_button_feedback_started_at = None;
                self.live_config_error = Some(error.to_string());
                self.push_notification(NotificationKind::Error, format!("保存失败：{error}"));
            }
        }
    }

    fn persist_config(&self) -> Result<(), AppError> {
        write_config(&self.config_path, &self.config)
    }

    fn sync_config_state(&mut self) {
        let stop_on_focus_lost = self.config.active_profile().stop_on_focus_lost;
        self.config.general.stop_on_focus_lost = stop_on_focus_lost;
    }

    fn validate_active_profile_for_persistence(
        &self,
        hotkey_registration: &HotkeyRegistration,
    ) -> Result<(), AppError> {
        let profile = self.config.active_profile();
        validate_config(profile)?;
        validate_bindable_input_action(&profile.action)?;
        hotkey_registration.validate()
    }

    fn is_capturing_shortcut(&self) -> bool {
        self.capture_state.is_some()
    }

    fn capture_is_active_for(&self, target: CaptureTarget) -> bool {
        self.capture_state
            .as_ref()
            .is_some_and(|capture| capture.target == target)
    }

    fn capture_preview_for(&self, target: CaptureTarget) -> Option<&str> {
        self.capture_state
            .as_ref()
            .filter(|capture| capture.target == target)
            .and_then(|capture| capture.preview_label.as_deref())
    }

    fn toggle_capture(&mut self, target: CaptureTarget) {
        if self.capture_is_active_for(target) {
            self.capture_state = None;
            return;
        }

        self.capture_state = Some(ShortcutCaptureState {
            target,
            preview_label: None,
            last_valid_label: None,
            saw_pressed_keys: false,
            paused_by_focus_loss: false,
        });
    }

    fn keyboard_action_label(action: &InputAction) -> Option<String> {
        match action {
            InputAction::MouseClick { .. } => None,
            InputAction::KeyPress { key_code } => Some(key_code.clone()),
            InputAction::KeyCombo {
                modifiers,
                key_code,
            } => {
                let mut parts = modifiers.clone();
                parts.push(key_code.clone());
                Some(parts.join("+"))
            }
        }
    }

    fn action_from_captured_label(label: &str) -> InputAction {
        let parts: Vec<String> = label
            .split('+')
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(ToOwned::to_owned)
            .collect();

        match parts.as_slice() {
            [] => InputAction::KeyPress {
                key_code: String::from("Space"),
            },
            [single] => InputAction::KeyPress {
                key_code: single.clone(),
            },
            _ => InputAction::KeyCombo {
                modifiers: parts[..parts.len() - 1].to_vec(),
                key_code: parts[parts.len() - 1].clone(),
            },
        }
    }

    fn apply_captured_value(&mut self, target: CaptureTarget, label: String) {
        match target {
            CaptureTarget::Action => {
                self.config.active_profile_mut().action = Self::action_from_captured_label(&label);
            }
            CaptureTarget::Hotkey(target) => {
                let hotkeys = &mut self.config.active_profile_mut().hotkeys;

                match target {
                    HotkeyField::Start => hotkeys.start = label,
                    HotkeyField::Stop => hotkeys.stop = label,
                    HotkeyField::Panic => hotkeys.panic = Some(label),
                }
            }
        }
    }

    fn advance_capture_state(
        capture: &mut ShortcutCaptureState,
        snapshot: Option<crate::platform::windows::input::HotkeyCapture>,
    ) -> Option<(CaptureTarget, String)> {
        match snapshot {
            Some(chord) => {
                capture.saw_pressed_keys = true;

                if chord.has_non_modifier_key && chord.is_valid_binding {
                    capture.preview_label = Some(chord.label.clone());
                    capture.last_valid_label = Some(chord.label);
                } else if capture.last_valid_label.is_none() {
                    capture.preview_label = Some(chord.label);
                }

                None
            }
            None if capture.saw_pressed_keys => {
                if let Some(label) = capture.last_valid_label.clone() {
                    Some((capture.target, label))
                } else {
                    capture.preview_label = None;
                    capture.saw_pressed_keys = false;
                    None
                }
            }
            None => None,
        }
    }

    fn reset_capture_progress(capture: &mut ShortcutCaptureState) {
        capture.preview_label = None;
        capture.last_valid_label = None;
        capture.saw_pressed_keys = false;
    }

    fn capture_can_poll_input(ctx: &egui::Context) -> bool {
        match current_foreground_window() {
            Some(window) => window.belongs_to_current_process(),
            None => ctx.input(|input| input.focused),
        }
    }

    fn update_capture_state(&mut self, ctx: &egui::Context) {
        if self.capture_state.is_none() {
            return;
        }

        ctx.request_repaint_after(Duration::from_millis(16));

        let can_poll_input = Self::capture_can_poll_input(ctx);
        if can_poll_input
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            self.capture_state = None;
            return;
        }

        let mut completed_capture = None;

        if let Some(capture) = &mut self.capture_state {
            if !can_poll_input {
                if !capture.paused_by_focus_loss {
                    Self::reset_capture_progress(capture);
                    capture.paused_by_focus_loss = true;
                }
                return;
            }

            if capture.paused_by_focus_loss {
                Self::reset_capture_progress(capture);
                capture.paused_by_focus_loss = false;
            }

            let snapshot = poll_hotkey_capture();
            completed_capture = Self::advance_capture_state(capture, snapshot);
        }

        if let Some((target, label)) = completed_capture {
            self.capture_state = None;
            self.apply_captured_value(target, label);
        }
    }

    fn handle_keyboard_shortcuts(&mut self, ctx: &egui::Context) {
        if self.is_capturing_shortcut() {
            return;
        }

        let should_save =
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if should_save {
            self.save_config();
        }
    }

    fn can_accept_stop(&self) -> bool {
        self.started_running_at
            .map(|started_at| started_at.elapsed() >= Duration::from_millis(250))
            .unwrap_or(true)
    }

    fn queue_action(&mut self, action: PendingUiAction) {
        self.pending_action = Some(action);
    }

    fn process_pending_action(&mut self) {
        let Some(action) = self.pending_action.take() else {
            return;
        };

        self.capture_state = None;

        match action {
            PendingUiAction::Start => self.start_engine(StartOrigin::UiButton),
            PendingUiAction::Stop => {
                if self.is_running() && self.can_accept_stop() {
                    let completed_actions = self.engine.completed_actions();
                    self.stop_engine();
                    self.push_notification(
                        NotificationKind::Info,
                        format!("已停止。本次一共执行了 {} 次。", completed_actions),
                    );
                }
            }
            PendingUiAction::CancelStart => self.cancel_delayed_start(),
            PendingUiAction::Save => self.save_config(),
        }
    }

    fn process_delayed_start(&mut self, ctx: &egui::Context) {
        let Some(start_at) = self.delayed_start_at else {
            return;
        };

        let now = Instant::now();
        if now >= start_at {
            self.delayed_start_at = None;
            self.start_engine_now();
        } else {
            ctx.request_repaint_after(start_at.saturating_duration_since(now));
        }
    }

    fn pump_focus_stop_guard(&mut self, ctx: &egui::Context) {
        if !self.is_running() {
            self.clear_focus_stop_guard();
            return;
        }

        let Some(mut guard) = self.focus_stop_guard else {
            return;
        };

        ctx.request_repaint_after(Duration::from_millis(100));

        let current_window = current_foreground_window();

        if guard.target_window_handle.is_none() {
            if let Some(window) =
                current_window.filter(|window| !window.belongs_to_current_process())
            {
                guard.target_window_handle = Some(window.handle);
                self.focus_stop_guard = Some(guard);
            }
            return;
        }

        self.focus_stop_guard = Some(guard);

        if current_window.map(|window| window.handle) != guard.target_window_handle {
            let completed_actions = self.engine.completed_actions();
            self.stop_engine();
            self.push_notification(
                NotificationKind::Info,
                format!(
                    "目标窗口失去焦点，已自动停止。本次一共执行了 {} 次。",
                    completed_actions
                ),
            );
        }
    }

    fn pump_engine(&mut self, ctx: &egui::Context) {
        if !self.is_running() {
            return;
        }

        let interval = Duration::from_millis(self.runtime_profile().interval_ms.max(1));
        let now = Instant::now();
        let last_tick = self.last_tick_at.unwrap_or(now - interval);
        let elapsed = now.saturating_duration_since(last_tick);

        if elapsed >= interval {
            match self.engine.tick() {
                Ok(true) => {
                    self.last_tick_at = Some(now);
                    if !self.is_running() {
                        self.snapshot_last_run_metrics();
                        self.last_tick_at = None;
                        self.started_running_at = None;
                        self.clear_focus_stop_guard();
                        self.reconcile_live_config();
                        self.push_notification(
                            NotificationKind::Info,
                            format!("运行结束，共执行 {} 次。", self.last_completed_actions),
                        );
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    self.last_tick_at = None;
                    self.push_notification(
                        NotificationKind::Error,
                        format!("执行循环出错：{error}"),
                    );
                }
            }
        }

        if self.is_running() {
            let remaining = interval.saturating_sub(elapsed);
            ctx.request_repaint_after(remaining);
        }
    }

    fn pump_hotkeys(&mut self, ctx: &egui::Context) {
        if self.is_capturing_shortcut() {
            return;
        }

        if ctx.wants_keyboard_input() {
            return;
        }

        let snapshot = self.hotkeys.poll();
        let trigger_mode = self.config.active_profile().trigger_mode;

        if snapshot.panic_pressed {
            self.stop_engine();
            self.push_notification(
                NotificationKind::Error,
                match self.hotkeys.panic_label() {
                    Some(label) => format!("已触发紧急停止：{label}"),
                    None => String::from("已触发紧急停止。"),
                },
            );
            return;
        }

        if snapshot.stop_pressed && self.is_running() && self.can_accept_stop() {
            self.stop_engine();
            self.push_notification(NotificationKind::Info, "已通过全局停止热键结束运行。");
            return;
        }

        match trigger_mode {
            TriggerMode::Toggle => {
                if snapshot.start_pressed && !self.is_running() {
                    self.start_engine(StartOrigin::Hotkey);
                }
            }
            TriggerMode::Hold => {
                if snapshot.start_pressed && !self.is_running() {
                    self.start_engine(StartOrigin::Hotkey);
                } else if !snapshot.start_down && self.is_running() && self.can_accept_stop() {
                    self.stop_engine();
                    self.push_notification(NotificationKind::Info, "已松开按住热键，运行已停止。");
                }
            }
        }
    }

    fn render_operation_bar(&mut self, ui: &mut egui::Ui) {
        let is_running = self.is_running();
        let is_pending_start = self.has_pending_start();
        let can_start = self.live_config_error.is_none();
        let can_stop = is_running && self.can_accept_stop();
        let countdown_text = self.pending_start_countdown_text();

        ui.spacing_mut().item_spacing = egui::vec2(18.0, 12.0);

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("OxyClick")
                        .size(32.0)
                        .strong()
                        .color(theme::text_primary()),
                );
                ui.label(
                    RichText::new("表单修改会自动用于下一次启动，按 Ctrl+S 保存到本地。")
                        .size(14.0)
                        .color(theme::text_secondary()),
                );

                if let Some(error) = &self.live_config_error {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(format!("当前表单暂不能启动：{error}"))
                            .size(13.0)
                            .color(theme::danger()),
                    );
                }
            });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let save_feedback_progress = self.save_button_feedback_progress();
                let save_button_fill = save_feedback_progress
                    .map(|progress| Self::lerp_color(theme::primary(), theme::surface(), progress))
                    .unwrap_or_else(theme::surface);
                let save_button_stroke = save_feedback_progress
                    .map(|progress| {
                        Self::lerp_color(theme::primary_dark(), theme::border(), progress)
                    })
                    .unwrap_or_else(theme::border);
                let save_button_text = save_feedback_progress
                    .map(|progress| {
                        Self::lerp_color(Color32::WHITE, theme::text_primary(), progress)
                    })
                    .unwrap_or_else(theme::text_primary);
                let save_button_label = if save_feedback_progress.is_some() {
                    "保存中"
                } else {
                    "保存到本地"
                };
                let save_button = egui::Button::new(
                    RichText::new(save_button_label)
                        .strong()
                        .color(save_button_text),
                )
                .min_size(egui::vec2(132.0, 46.0))
                .fill(save_button_fill)
                .stroke(egui::Stroke::new(1.0, save_button_stroke));
                if ui.add(save_button).clicked() {
                    self.queue_action(PendingUiAction::Save);
                }

                ui.add_space(10.0);

                let (action_label, fill_color, stroke_color) = if is_pending_start {
                    (
                        countdown_text
                            .as_ref()
                            .map(|text| format!("取消启动 {text}"))
                            .unwrap_or_else(|| String::from("取消启动")),
                        theme::warning(),
                        theme::warning_dark(),
                    )
                } else if is_running {
                    (
                        String::from("停止运行"),
                        theme::danger(),
                        theme::danger_dark(),
                    )
                } else {
                    (
                        String::from("开始运行"),
                        theme::primary(),
                        theme::primary_dark(),
                    )
                };

                let action_button = egui::Button::new(
                    RichText::new(action_label)
                        .size(22.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .min_size(egui::vec2(208.0, 66.0))
                .fill(fill_color)
                .stroke(egui::Stroke::new(1.5, stroke_color));

                let action_response = if is_pending_start {
                    ui.add(action_button)
                } else if is_running {
                    ui.add_enabled(can_stop, action_button)
                } else {
                    ui.add_enabled(can_start, action_button)
                };

                if action_response.clicked() {
                    if is_pending_start {
                        self.queue_action(PendingUiAction::CancelStart);
                    } else if is_running && can_stop {
                        self.queue_action(PendingUiAction::Stop);
                    } else if !is_running && can_start {
                        self.queue_action(PendingUiAction::Start);
                    }
                }
            });
        });
    }

    fn render_profile_editor(&mut self, ui: &mut egui::Ui, target_height: f32) -> f32 {
        let mut profile = self.config.active_profile().clone();

        let (_, metrics) = card_with_body_spacing_and_min_height_and_metrics(
            ui,
            "配置编辑",
            "表单修改会自动更新到内存，下一次启动会直接使用；按 Ctrl+S 保存到本地。",
            2.0,
            (target_height - theme::CARD_PADDING * 2.0).max(0.0),
            |ui| {
                ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);
                form_grid(ui, "profile_editor_grid", |ui| {
                    form_row(ui, "触发模式", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
                            ui.selectable_value(
                                &mut profile.trigger_mode,
                                TriggerMode::Toggle,
                                "切换",
                            );
                            ui.selectable_value(
                                &mut profile.trigger_mode,
                                TriggerMode::Hold,
                                "按住",
                            );
                        });
                    });

                    let mut run_mode_kind = match profile.run_mode {
                        RunMode::Infinite => 0_u8,
                        RunMode::Count { .. } => 1_u8,
                        RunMode::Timed { .. } => 2_u8,
                    };

                    form_row(ui, "运行模式", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
                            ui.selectable_value(&mut run_mode_kind, 0, "无限");
                            ui.selectable_value(&mut run_mode_kind, 1, "计数");
                            ui.selectable_value(&mut run_mode_kind, 2, "限时");
                        });
                    });

                    match (&mut profile.run_mode, run_mode_kind) {
                        (RunMode::Infinite, 0) => {}
                        (RunMode::Count { .. }, 1) => {}
                        (RunMode::Timed { .. }, 2) => {}
                        (_, 0) => profile.run_mode = RunMode::Infinite,
                        (_, 1) => profile.run_mode = RunMode::Count { total: 100 },
                        (_, 2) => profile.run_mode = RunMode::Timed { duration_ms: 5000 },
                        _ => {}
                    }

                    match &mut profile.run_mode {
                        RunMode::Infinite => {}
                        RunMode::Count { total } => {
                            number_row(ui, "次数", total, " 次", 1.0);
                        }
                        RunMode::Timed { duration_ms } => {
                            number_row(ui, "时长", duration_ms, " ms", 50.0);
                        }
                    }

                    let mut action_kind = match &profile.action {
                        InputAction::MouseClick { button } => match button {
                            MouseButton::Left => 0_u8,
                            MouseButton::Right => 1_u8,
                            MouseButton::Middle => 2_u8,
                        },
                        InputAction::KeyPress { .. } | InputAction::KeyCombo { .. } => 3_u8,
                    };

                    form_row(ui, "动作类型", |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(12.0, 10.0);
                            ui.selectable_value(&mut action_kind, 0, "鼠标左键");
                            ui.selectable_value(&mut action_kind, 1, "鼠标右键");
                            ui.selectable_value(&mut action_kind, 2, "鼠标中键");
                            ui.selectable_value(&mut action_kind, 3, "键盘按键");
                        });
                    });

                    match action_kind {
                        0 => {
                            profile.action = InputAction::MouseClick {
                                button: MouseButton::Left,
                            };
                        }
                        1 => {
                            profile.action = InputAction::MouseClick {
                                button: MouseButton::Right,
                            };
                        }
                        2 => {
                            profile.action = InputAction::MouseClick {
                                button: MouseButton::Middle,
                            };
                        }
                        3 => {
                            if matches!(profile.action, InputAction::MouseClick { .. }) {
                                profile.action = InputAction::KeyPress {
                                    key_code: String::from("Space"),
                                };
                            }
                        }
                        _ => {}
                    }

                    if action_kind != 3 && self.capture_is_active_for(CaptureTarget::Action) {
                        self.capture_state = None;
                    }

                    match &profile.action {
                        InputAction::MouseClick { .. } => {}
                        InputAction::KeyPress { .. } | InputAction::KeyCombo { .. } => {
                            form_row(ui, "按键录制", |ui| {
                                let is_recording =
                                    self.capture_is_active_for(CaptureTarget::Action);
                                let display_value = if is_recording {
                                    self.capture_preview_for(CaptureTarget::Action)
                                        .map(str::to_owned)
                                } else {
                                    Self::keyboard_action_label(&profile.action)
                                };
                                let placeholder = if is_recording {
                                    "请直接按下按键"
                                } else {
                                    "点击后直接按键录制"
                                };

                                let response = hotkey_capture_field(
                                    ui,
                                    display_value.as_deref(),
                                    placeholder,
                                    is_recording,
                                );

                                if is_recording && response.clicked_elsewhere() {
                                    self.capture_state = None;
                                } else if response.clicked() {
                                    self.toggle_capture(CaptureTarget::Action);
                                }
                            });

                            form_note_row(
                                ui,
                                "单键和组合键统一在这里录制。\n单独按 Ctrl / Alt / Shift / Win 不会保存；需要再配合其他键。\n常规键只能有一个；Ctrl / Alt / Shift / Win 可以多个并作为修饰键。\nTab / Esc 不会保存，Esc 可立即取消当前录制。\n切到其他程序窗口时会暂停录制，回到本程序窗口后继续。\n点击输入框后直接按键，点击其他位置可取消。",
                            );
                        }
                    }

                    number_row(ui, "间隔", &mut profile.interval_ms, " ms", 1.0);
                    number_row(ui, "按下时长", &mut profile.press_duration_ms, " ms", 1.0);

                    let jitter = profile.jitter_ms.get_or_insert(0);
                    number_row(ui, "抖动", jitter, " ms", 1.0);

                    form_row(ui, "启动前延迟", |ui| {
                        number_field(ui, &mut profile.start_delay_ms, " ms", 50.0);
                    });
                    form_note_row(
                        ui,
                        "仅在通过界面上的“开始运行”时生效。\n用于预留切回目标窗口或移开鼠标的时间；设为 0 可关闭。",
                    );
                });
            },
        );

        *self.config.active_profile_mut() = profile;
        metrics.actual_height
    }

    fn render_hotkeys(&mut self, ui: &mut egui::Ui, target_height: f32) -> f32 {
        let mut minimize_to_tray = self.config.general.minimize_to_tray;
        let start_hotkey = self.config.active_profile().hotkeys.start.clone();
        let stop_hotkey = self.config.active_profile().hotkeys.stop.clone();
        let mut panic_hotkey = self.config.active_profile().hotkeys.panic.clone();
        let mut stop_on_focus_lost = self.config.active_profile().stop_on_focus_lost;

        let (_, metrics) = card_with_body_spacing_and_min_height_and_metrics(
            ui,
            "热键与安全",
            "点击输入框后直接按下快捷键，热键会在表单有效时自动生效。",
            8.0,
            (target_height - theme::CARD_PADDING * 2.0).max(0.0),
            |ui| {
                ui.spacing_mut().button_padding = egui::vec2(10.0, 6.0);

                form_grid(ui, "hotkeys_grid", |ui| {
                    form_row(ui, "开始热键", |ui| {
                        let is_recording =
                            self.capture_is_active_for(CaptureTarget::Hotkey(HotkeyField::Start));
                        let display_value = if is_recording {
                            self.capture_preview_for(CaptureTarget::Hotkey(HotkeyField::Start))
                                .map(str::to_owned)
                        } else {
                            Some(start_hotkey.clone())
                        };
                        let placeholder = if is_recording {
                            "请直接按下快捷键"
                        } else {
                            "点击后直接按键录制"
                        };

                        let response = hotkey_capture_field(
                            ui,
                            display_value.as_deref(),
                            placeholder,
                            is_recording,
                        );

                        if is_recording && response.clicked_elsewhere() {
                            self.capture_state = None;
                        } else if response.clicked() {
                            self.toggle_capture(CaptureTarget::Hotkey(HotkeyField::Start));
                        }
                    });

                    form_row(ui, "停止热键", |ui| {
                        let is_recording =
                            self.capture_is_active_for(CaptureTarget::Hotkey(HotkeyField::Stop));
                        let display_value = if is_recording {
                            self.capture_preview_for(CaptureTarget::Hotkey(HotkeyField::Stop))
                                .map(str::to_owned)
                        } else {
                            Some(stop_hotkey.clone())
                        };
                        let placeholder = if is_recording {
                            "请直接按下快捷键"
                        } else {
                            "点击后直接按键录制"
                        };

                        let response = hotkey_capture_field(
                            ui,
                            display_value.as_deref(),
                            placeholder,
                            is_recording,
                        );

                        if is_recording && response.clicked_elsewhere() {
                            self.capture_state = None;
                        } else if response.clicked() {
                            self.toggle_capture(CaptureTarget::Hotkey(HotkeyField::Stop));
                        }
                    });

                    form_row(ui, "紧急停止", |ui| {
                        let is_recording =
                            self.capture_is_active_for(CaptureTarget::Hotkey(HotkeyField::Panic));
                        let display_value = if is_recording {
                            self.capture_preview_for(CaptureTarget::Hotkey(HotkeyField::Panic))
                                .map(str::to_owned)
                        } else {
                            panic_hotkey.clone()
                        };
                        let placeholder = if is_recording {
                            "请直接按下快捷键"
                        } else {
                            "点击后直接按键录制"
                        };

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);

                            let response = hotkey_capture_field(
                                ui,
                                display_value.as_deref(),
                                placeholder,
                                is_recording,
                            );

                            if is_recording && response.clicked_elsewhere() {
                                self.capture_state = None;
                            } else if response.clicked() {
                                self.toggle_capture(CaptureTarget::Hotkey(HotkeyField::Panic));
                            }

                            let clear_button = egui::Button::new("清空")
                                .min_size(egui::vec2(54.0, 30.0))
                                .fill(theme::surface())
                                .stroke(egui::Stroke::new(1.0, theme::border()));
                            if ui
                                .add_enabled(panic_hotkey.is_some(), clear_button)
                                .clicked()
                            {
                                panic_hotkey = None;
                                if is_recording {
                                    self.capture_state = None;
                                }
                            }
                        });
                    });

                    form_note_row(
                        ui,
                        "点击输入框后直接按下快捷键。\n单独按 Ctrl / Alt / Shift / Win 不会保存；需要再配合其他键。\n常规键只能有一个；Ctrl / Alt / Shift / Win 可以多个并作为修饰键。\nTab / Esc 不会保存，Esc 可立即取消当前录制。\n切到其他程序窗口时会暂停录制，回到本程序窗口后继续。\n再次点击当前输入框或点击其他位置可取消录制，紧急停止支持清空。",
                    );

                    form_row(ui, "安全选项", |ui| {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(10.0, 10.0);
                            ui.checkbox(&mut stop_on_focus_lost, "目标窗口失去焦点时自动停止");
                            ui.checkbox(
                                &mut minimize_to_tray,
                                "后续接入托盘功能后允许最小化到托盘",
                            );
                        });
                    });
                });
            },
        );

        self.config.general.minimize_to_tray = minimize_to_tray;
        let profile = self.config.active_profile_mut();
        profile.hotkeys.panic = panic_hotkey;
        profile.stop_on_focus_lost = stop_on_focus_lost;
        metrics.actual_height
    }

    fn render_dashboard(&self, ui: &mut egui::Ui) {
        let state_label = if self.is_running() {
            String::from("运行中")
        } else {
            String::from("待运行")
        };
        let state_color = if self.is_running() {
            theme::danger()
        } else {
            theme::primary()
        };
        let status = StatusPanelData {
            state_label,
            state_color,
            completed_actions: self.displayed_completed_actions(),
            elapsed_label: Self::format_elapsed_duration(self.displayed_run_duration()),
        };

        render_status_panel(ui, &status);
    }

    fn run_cards_layout_pass(&mut self, viewport_size: egui::Vec2) -> (f32, f32) {
        let ctx = egui::Context::default();
        configure_visuals(&ctx);
        ctx.begin_frame(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size)),
            ..Default::default()
        });

        let mut profile_editor_height = 0.0;
        let mut hotkeys_height = 0.0;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::app_background())
                    .inner_margin(egui::Margin::same(theme::PANEL_MARGIN)),
            )
            .show(&ctx, |ui| {
                self.render_dashboard(ui);
                ui.add_space(theme::SECTION_GAP);

                let gap = theme::SECTION_GAP;
                let target_card_height = self.cards_display_height;

                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    ui.columns(2, |columns| {
                        profile_editor_height =
                            self.render_profile_editor(&mut columns[0], target_card_height);
                        let synced_target_height = target_card_height.max(profile_editor_height);
                        hotkeys_height = self.render_hotkeys(&mut columns[1], synced_target_height);
                    });
                });
            });

        let _ = ctx.end_frame();
        (profile_editor_height, hotkeys_height)
    }

    fn collect_layout_debug_metrics(&mut self, viewport_size: egui::Vec2) -> LayoutDebugMetrics {
        self.cards_display_height = 0.0;
        self.profile_editor_actual_height = 0.0;
        self.hotkeys_actual_height = 0.0;

        for _ in 0..3 {
            let (profile_editor_height, hotkeys_height) = self.run_cards_layout_pass(viewport_size);
            self.profile_editor_actual_height = profile_editor_height;
            self.hotkeys_actual_height = hotkeys_height;
            self.cards_display_height = profile_editor_height.max(hotkeys_height);
        }

        LayoutDebugMetrics {
            profile_editor_actual_height: self.profile_editor_actual_height,
            hotkeys_actual_height: self.hotkeys_actual_height,
            cards_display_height: self.cards_display_height,
            profile_editor_fill_height: (self.cards_display_height
                - self.profile_editor_actual_height)
                .max(0.0),
            hotkeys_fill_height: (self.cards_display_height - self.hotkeys_actual_height).max(0.0),
        }
    }

    fn sync_window_height(&self, ctx: &egui::Context, top_bar_height: f32, content_height: f32) {
        let (current_inner_size, monitor_size, maximized, fullscreen) = ctx.input(|input| {
            let viewport = input.viewport();
            (
                viewport.inner_rect.map(|rect| rect.size()),
                viewport.monitor_size,
                viewport.maximized.unwrap_or(false),
                viewport.fullscreen.unwrap_or(false),
            )
        });

        if maximized || fullscreen {
            return;
        }

        let current_inner_size = current_inner_size
            .unwrap_or_else(|| egui::vec2(DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT));

        let mut desired_height = top_bar_height + content_height + theme::PANEL_MARGIN * 2.0;
        desired_height = desired_height.max(MIN_WINDOW_HEIGHT);

        if let Some(monitor_size) = monitor_size {
            desired_height = desired_height.min(monitor_size.y.max(MIN_WINDOW_HEIGHT));
        }

        if (current_inner_size.y - desired_height).abs() <= 1.0 {
            return;
        }

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
            current_inner_size.x.max(MIN_WINDOW_WIDTH),
            desired_height,
        )));
    }
}

pub fn run() -> Result<(), AppError> {
    let mut application = Application::bootstrap()?;
    if std::env::args().any(|arg| arg == "--headless-summary") {
        println!("{}", application.summary());
        return Ok(());
    }
    if std::env::args().any(|arg| arg == "--layout-debug") {
        let metrics = application.collect_layout_debug_metrics(egui::vec2(1120.0, 680.0));
        println!(
            "profile_editor_actual_height={:.1}",
            metrics.profile_editor_actual_height
        );
        println!("hotkeys_actual_height={:.1}", metrics.hotkeys_actual_height);
        println!("cards_display_height={:.1}", metrics.cards_display_height);
        println!(
            "profile_editor_fill_height={:.1}",
            metrics.profile_editor_fill_height
        );
        println!("hotkeys_fill_height={:.1}", metrics.hotkeys_fill_height);
        return Ok(());
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT])
            .with_min_inner_size([MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT]),
        ..Default::default()
    };

    eframe::run_native(
        "OxyClick",
        native_options,
        Box::new(|creation_context| {
            configure_visuals(&creation_context.egui_ctx);
            let app = Application::bootstrap()
                .map_err(|error| Box::new(error) as Box<dyn Error + Send + Sync>)?;
            Ok(Box::new(app))
        }),
    )
    .map_err(|error| AppError::Unsupported(format!("界面运行失败：{error}")))?;

    Ok(())
}

fn configure_visuals(ctx: &egui::Context) {
    configure_fonts(ctx);
    theme::apply(ctx);
}

fn configure_fonts(ctx: &egui::Context) {
    let Some(font_bytes) = load_cjk_font_bytes() else {
        return;
    };

    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        String::from("system_cjk"),
        egui::FontData::from_owned(font_bytes),
    );

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        family.insert(0, String::from("system_cjk"));
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        family.insert(0, String::from("system_cjk"));
    }

    ctx.set_fonts(fonts);
}

fn load_cjk_font_bytes() -> Option<Vec<u8>> {
    let fonts_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");

    let candidates = [
        fonts_dir.join("simhei.ttf"),
        fonts_dir.join("msyh.ttc"),
        fonts_dir.join("msyhbd.ttc"),
        fonts_dir.join("simsun.ttc"),
    ];

    candidates.iter().find_map(|path| fs::read(path).ok())
}

#[cfg(test)]
mod tests {
    use super::{Application, CaptureTarget, HotkeyField, ShortcutCaptureState};
    use crate::platform::windows::input::HotkeyCapture;

    #[test]
    fn capture_keeps_last_valid_combo_until_release() {
        let mut capture = ShortcutCaptureState {
            target: CaptureTarget::Hotkey(HotkeyField::Start),
            preview_label: None,
            last_valid_label: None,
            saw_pressed_keys: false,
            paused_by_focus_loss: false,
        };

        assert_eq!(
            Application::advance_capture_state(
                &mut capture,
                Some(HotkeyCapture {
                    label: String::from("Ctrl+Alt+K"),
                    has_non_modifier_key: true,
                    is_valid_binding: true,
                }),
            ),
            None
        );
        assert_eq!(capture.preview_label.as_deref(), Some("Ctrl+Alt+K"));

        assert_eq!(
            Application::advance_capture_state(
                &mut capture,
                Some(HotkeyCapture {
                    label: String::from("Ctrl+Alt"),
                    has_non_modifier_key: false,
                    is_valid_binding: false,
                }),
            ),
            None
        );
        assert_eq!(capture.preview_label.as_deref(), Some("Ctrl+Alt+K"));

        assert_eq!(
            Application::advance_capture_state(&mut capture, None),
            Some((
                CaptureTarget::Hotkey(HotkeyField::Start),
                String::from("Ctrl+Alt+K")
            ))
        );
    }

    #[test]
    fn modifier_only_capture_does_not_commit() {
        let mut capture = ShortcutCaptureState {
            target: CaptureTarget::Action,
            preview_label: None,
            last_valid_label: None,
            saw_pressed_keys: false,
            paused_by_focus_loss: false,
        };

        assert_eq!(
            Application::advance_capture_state(
                &mut capture,
                Some(HotkeyCapture {
                    label: String::from("Ctrl+Shift"),
                    has_non_modifier_key: false,
                    is_valid_binding: false,
                }),
            ),
            None
        );
        assert_eq!(capture.preview_label.as_deref(), Some("Ctrl+Shift"));

        assert_eq!(Application::advance_capture_state(&mut capture, None), None);
        assert_eq!(capture.preview_label, None);
        assert!(capture.last_valid_label.is_none());
    }

    #[test]
    fn reset_capture_progress_clears_partial_recording_state() {
        let mut capture = ShortcutCaptureState {
            target: CaptureTarget::Action,
            preview_label: Some(String::from("Ctrl+K")),
            last_valid_label: Some(String::from("Ctrl+K")),
            saw_pressed_keys: true,
            paused_by_focus_loss: false,
        };

        Application::reset_capture_progress(&mut capture);

        assert_eq!(capture.preview_label, None);
        assert_eq!(capture.last_valid_label, None);
        assert!(!capture.saw_pressed_keys);
    }

    #[test]
    fn invalid_multi_regular_capture_does_not_commit() {
        let mut capture = ShortcutCaptureState {
            target: CaptureTarget::Hotkey(HotkeyField::Start),
            preview_label: None,
            last_valid_label: None,
            saw_pressed_keys: false,
            paused_by_focus_loss: false,
        };

        assert_eq!(
            Application::advance_capture_state(
                &mut capture,
                Some(HotkeyCapture {
                    label: String::from("Ctrl+A+1"),
                    has_non_modifier_key: true,
                    is_valid_binding: false,
                }),
            ),
            None
        );
        assert_eq!(capture.preview_label.as_deref(), Some("Ctrl+A+1"));
        assert!(capture.last_valid_label.is_none());

        assert_eq!(Application::advance_capture_state(&mut capture, None), None);
    }
}

impl eframe::App for Application {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.expire_notification();
        self.expire_save_button_feedback();
        let config_before = self.config.clone();

        if self.is_capturing_shortcut() {
            self.update_capture_state(ctx);
        } else {
            self.handle_keyboard_shortcuts(ctx);
            self.pump_hotkeys(ctx);
        }

        self.process_delayed_start(ctx);
        self.pump_focus_stop_guard(ctx);
        self.pump_engine(ctx);

        if self.save_button_feedback_started_at.is_some() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        if !self.is_running() {
            ctx.request_repaint_after(Duration::from_millis(16));
        }

        let (top_fill, top_stroke) = if self.is_running() {
            (
                theme::danger_soft(),
                egui::Stroke::new(1.6, theme::danger_soft_border()),
            )
        } else if self.live_config_error.is_some() {
            (
                theme::warning_soft(),
                egui::Stroke::new(1.2, theme::warning_soft_border()),
            )
        } else {
            (
                theme::app_background(),
                egui::Stroke::new(1.0, theme::border()),
            )
        };

        let top_bar_response = egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::default()
                    .fill(top_fill)
                    .stroke(top_stroke)
                    .inner_margin(egui::Margin::same(theme::PANEL_MARGIN)),
            )
            .show(ctx, |ui| self.render_operation_bar(ui));

        if self.config != config_before {
            self.reconcile_live_config();
        }

        self.process_pending_action();

        let mut content_height = 0.0;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(theme::app_background())
                    .inner_margin(egui::Margin::same(theme::PANEL_MARGIN)),
            )
            .show(ctx, |ui| {
                let content_response = ui.vertical(|ui| {
                    self.render_dashboard(ui);
                    ui.add_space(theme::SECTION_GAP);

                    let gap = theme::SECTION_GAP;
                    let target_card_height = self.cards_display_height;
                    let mut profile_editor_height = 0.0;
                    let mut hotkeys_height = 0.0;

                    ui.scope(|ui| {
                        ui.spacing_mut().item_spacing.x = gap;
                        ui.columns(2, |columns| {
                            profile_editor_height =
                                self.render_profile_editor(&mut columns[0], target_card_height);
                            let synced_target_height =
                                target_card_height.max(profile_editor_height);
                            hotkeys_height =
                                self.render_hotkeys(&mut columns[1], synced_target_height);
                        });
                    });

                    let next_cards_display_height = profile_editor_height.max(hotkeys_height);
                    let heights_changed =
                        (self.profile_editor_actual_height - profile_editor_height).abs() > 0.5
                            || (self.hotkeys_actual_height - hotkeys_height).abs() > 0.5
                            || (self.cards_display_height - next_cards_display_height).abs() > 0.5;

                    self.cards_display_height = next_cards_display_height;
                    self.profile_editor_actual_height = profile_editor_height;
                    self.hotkeys_actual_height = hotkeys_height;

                    if heights_changed {
                        ctx.request_repaint();
                    }
                });

                content_height = content_response.response.rect.height();
            });

        self.sync_window_height(ctx, top_bar_response.response.rect.height(), content_height);
    }
}
