# OxyClick 技术开发文档

## 1. 文档目的

本文档用于描述 OxyClick 当前版本的技术实现、模块边界、配置结构和后续演进方向。它不是纯规划文档，而是尽量与当前代码保持同步，方便继续开发和维护。

当前版本已经实现：

- Rust + `eframe/egui` 桌面 GUI
- Windows `SendInput` 输入注入
- 可自定义全局热键
- 低级键盘钩子优先、轮询回退的热键后端
- TOML 配置读写与旧配置迁移
- 中文界面、CJK 字体加载、Release 无控制台窗口

## 2. 当前实现快照

### 2.1 产品形态

- 平台：Windows 10/11
- 形态：本地桌面 GUI 工具
- 语言：中文优先
- 主要用途：鼠标连点、键盘连发、自动化输入测试

### 2.2 已实现能力

- 鼠标左键、右键、中键点击
- 键盘单键与组合键输入（组合键规则为“多个修饰键 + 一个常规键”）
- 配置编辑中的键盘动作统一通过录制框设置
- `无限 / 计数 / 限时` 运行模式
- `切换 / 按住` 触发模式
- 默认热键 `F6 / F7 / Ctrl+Alt+Pause`
- 自定义热键并即时用于下一次启动
- 热键输入框支持直接录制组合键；单独按 `Ctrl / Alt / Shift / Win` 不会提交；`Tab / Esc` 不会保存
- `Ctrl+S` / “保存到本地” 持久化配置
- 保存触发后按钮有 1 秒“保存中”渐变反馈
- 通过界面启动时的启动前延迟倒计时
- 目标窗口失去前台焦点时自动停止
- 运行状态、执行次数、执行时长展示
- 配置校验、热键冲突校验、旧配置迁移

### 2.3 当前未完成但已预留

- 独立执行线程
- 图形化日志面板
- 多配置数据结构已经保留，后续计划补多预设管理界面

## 3. 技术选型

| 领域 | 当前方案 | 说明 |
| --- | --- | --- |
| 语言 | Rust 2021 | 类型安全、适合状态控制和桌面工具 |
| GUI | `eframe / egui` | 迭代快，适合工具型应用 |
| 输入注入 | Windows `SendInput` | 使用原生输入 API 发送真实输入事件 |
| 全局热键 | Low-Level Keyboard Hook + `GetAsyncKeyState` 回退 | 先尝试高兼容钩子，失败时退回轮询 |
| 配置序列化 | `serde + toml` | 配置可读、易迁移 |
| Windows API 绑定 | `windows-sys` | 直接访问底层 Win32 能力 |
| 中文字体 | 运行时加载系统 CJK 字体 | 避免中文乱码 |
| Release 形态 | `windows_subsystem = "windows"` | 发布版不弹出终端窗口 |

说明：

- 当前并未引入独立日志框架，错误和状态反馈主要通过 UI 提示与错误弹窗完成。
- Release 失败时会通过 `MessageBoxW` 显示启动错误。

## 4. 当前目录结构

```text
src/
  main.rs
  lib.rs
  app.rs
  error.rs
  config/
    file.rs
    mod.rs
    schema.rs
  core/
    mod.rs
    model.rs
    state.rs
    validate.rs
  engine/
    commands.rs
    mod.rs
    runner.rs
    scheduler.rs
  platform/
    mod.rs
    windows/
      hook.rs
      hotkey.rs
      input.rs
      mod.rs
  ui/
    mod.rs
    panels.rs
    theme.rs
    widgets.rs
config/
  config.toml
docs/
  TECH.md
```

## 5. 核心数据模型

### 5.1 触发模式

```rust
pub enum TriggerMode {
    Hold,
    Toggle,
}
```

### 5.2 运行模式

```rust
pub enum RunMode {
    Infinite,
    Count { total: u64 },
    Timed { duration_ms: u64 },
}
```

### 5.3 输入动作

```rust
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

pub enum InputAction {
    MouseClick { button: MouseButton },
    KeyPress { key_code: String },
    KeyCombo { modifiers: Vec<String>, key_code: String },
}
```

### 5.4 热键绑定

```rust
pub struct HotkeyBindings {
    pub start: String,
    pub stop: String,
    pub panic: Option<String>,
}
```

默认值：

- 开始：`F6`
- 停止：`F7`
- 紧急停止：`Ctrl+Alt+Pause`

当前 UI 中，三项热键都通过“点击后直接按键”的方式录制，不再依赖手动输入字符串。
录制规则统一为：`Ctrl / Alt / Shift / Win` 可多个并作为修饰键，字母 / 数字 / 符号键只能有一个常规键。

### 5.5 单个任务配置

```rust
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
```

默认值要点：

- `name = "默认配置"`
- `trigger_mode = Toggle`
- `run_mode = Infinite`
- `action = MouseClick(Left)`
- `start_delay_ms = 700`
- `interval_ms = 25`
- `press_duration_ms = 5`
- `jitter_ms = Some(0)`
- `stop_on_focus_lost = true`

### 5.6 全局配置

```rust
pub struct GeneralConfig {
    pub launch_on_startup: bool,
    pub stop_on_focus_lost: bool,
}

pub struct AppConfig {
    pub general: GeneralConfig,
    pub profiles: Vec<ClickTaskConfig>,
    pub active_profile_index: usize,
}
```

说明：

- 当前 UI 只编辑活动配置，但底层结构已经保留 `profiles` 列表。

## 6. 配置系统

### 6.1 配置文件位置

- 首选：`%APPDATA%\OxyClick\config.toml`
- 回退：项目目录 `config\config.toml`

启动时逻辑：

1. 优先读取 `%APPDATA%` 配置。
2. 如果该路径不可用或读取失败，再尝试便携配置路径。
3. 如果目标配置文件不存在，自动创建默认配置。

### 6.2 表单与持久化行为

- 表单修改后会立刻写入内存中的活动配置。
- 当前表单合法时，下一次启动直接使用最新内存配置。
- 只有点击“保存到本地”或按 `Ctrl+S` 时，才写入磁盘。
- 不再需要单独的“应用配置”按钮。
- 保存成功后，顶部“保存到本地”按钮会短暂切换为“保存中”，并在约 1 秒内渐变回默认样式。
- `name` 字段仍保留在配置模型中，但当前界面暂时隐藏“配置名称”编辑项，等待后续多预设管理场景再开放。

### 6.3 兼容性迁移

当前已实现的迁移包括：

- `Default Profile` 自动迁移为 `默认配置`
- `控制+Alt+暂停` 自动迁移为 `Ctrl+Alt+Pause`
- 缺失配置时自动补默认值，例如 `start_delay_ms`

### 6.4 配置示例

```toml
active_profile_index = 0

[general]
launch_on_startup = false
stop_on_focus_lost = true

[[profiles]]
name = "默认配置"
trigger_mode = "toggle"
start_delay_ms = 700
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
```

## 7. UI 与交互结构

### 7.1 当前界面布局

- 顶部操作区：
  - 标题 `OxyClick`
  - 表单说明
  - 大号主按钮 `开始运行 / 停止运行 / 取消启动`
  - “保存到本地”按钮
- 中部状态行：
  - 横向铺满内容区
  - 展示 `运行状态 / 执行次数 / 执行时长`
- 下方双卡片区域：
  - 左侧 `配置编辑`
  - 右侧 `热键与安全`

### 7.2 当前交互规则

- 表单输入会即时影响下一次启动配置。
- 键盘动作与热键都支持点击输入框后直接录制。
- 录制状态下，点击当前输入框以外的其他区域会立即退出录制。
- 录制状态下按 `Esc` 会立即取消本次录制，`Tab / Esc` 都不会保存为绑定结果。
- 如果切到其他程序窗口，录制会暂停；回到 OxyClick 窗口后继续等待新的录制输入。
- 组合键只允许“多个修饰键 + 一个常规键”；字母 / 数字 / 符号键之间不能互相组合成多主键。
- 运行中会根据当前状态切换主按钮颜色和文本。
- 如果表单非法，顶部会提示“当前表单暂不能启动”。
- 启动前倒计时期间，主按钮会变成“取消启动 xx ms / x.x 秒”。
- 点击“保存到本地”或按 `Ctrl+S` 后，保存按钮会短暂显示“保存中”并做渐变反馈。
- 如果启用了 `stop_on_focus_lost`，程序会在开始运行后锁定目标前台窗口；后续只要当前前台窗口不再是它，就会自动停止。
- “运行模式”和“动作类型”会驱动表单动态增减字段，例如：
  - `计数 / 限时` 模式会显示额外参数项
  - `键盘按键` 动作会显示录制框，鼠标动作会显示按钮选择

### 7.3 启动前延迟规则

`start_delay_ms` 只在以下条件下生效：

- 从界面上的“开始运行”按钮启动

设计原因：

- 预留切回目标窗口或移开鼠标的时间。
- 避免用鼠标点击“开始运行”后，程序刚开始发送真实输入就再次点到自己的按钮。
- 配合 `stop_on_focus_lost` 使用时，可以更稳定地锁定真正的目标窗口，而不是误锁到 OxyClick 自己。

### 7.4 当前视觉实现

- 浅色背景 + 蓝色主色
- 圆角卡片与按钮
- Windows 系统 CJK 字体优先加载
- Release 版不显示终端窗口

### 7.5 当前布局策略

- 状态行位于双卡片区域上方，单独占一整行。
- 左右两张卡片会测量各自的实际内容高度，并将较矮的一张补齐到较高者的展示高度。
- 主窗口会根据顶部操作区和内容区的总高度动态调整自身高度，减少默认状态下的无效留白。
- 双卡片的实际高度判断以卡片正文内容为准，而不是简单拉伸到父容器剩余空间。

## 8. 热键系统

### 8.1 解析与校验

热键字符串会解析成 `HotkeyChord`，并执行以下校验：

- 开始热键不能为空
- 停止热键不能为空
- 紧急停止热键可为空
- 开始与停止不能相同
- 紧急停止不能与开始、停止相同
- 不支持的按键名称会直接报错
- 单独的 `Ctrl / Alt / Shift / Win` 不能作为有效热键
- 每个热键只能有一个常规键；`Ctrl / Alt / Shift / Win` 可同时作为修饰键

### 8.2 运行时策略

当前热键后端分两层：

1. 优先安装 `Low-Level Keyboard Hook`
2. 失败时回退到 `GetAsyncKeyState` 轮询

这样做的目的：

- 常见桌面场景下尽量提升响应稳定性
- 在钩子不可用时，仍保留基础热键能力

### 8.3 UI 与热键关系

- 表单改动后会触发重新校验。
- 表单合法时，热键配置会用于下一次启动。
- 保存到本地只负责持久化，不影响“下一次启动使用最新内存配置”。
- 热键录制期间会暂时屏蔽全局热键监听和 `Ctrl+S`，避免录制过程误触发开始、停止或保存。
- 单独按 `Ctrl / Alt / Shift / Win` 不会提交录制结果，必须至少包含一个非修饰键。
- `Tab / Esc` 不会提交录制结果；`Esc` 会直接取消当前录制。
- 常规键只能有一个；例如 `A+1`、`1+.`、`Ctrl+A+1` 都会被视为无效组合。
- 再次点击当前录制框或点击界面其他位置时，会立即退出录制状态。
- 如果切到其他程序窗口，录制会暂停；回到本程序窗口后继续等待新的录制输入。
- 录制逻辑使用按键状态轮询，能直接捕获 `Ctrl / Alt / Shift / Win / Pause`、常用符号键与小键盘符号键等常见组合键。
- 在低级键盘钩子路径下，通用修饰键会兼容左右键，例如 `Ctrl` 能匹配左右 `Ctrl`。

## 9. 运行引擎与调度

### 9.1 当前实现方式

当前不是独立执行线程模型，而是：

- `eframe/egui` 主更新循环驱动界面
- `app::update` 中调用 `pump_engine`
- 通过 `ctx.request_repaint_after(...)` 控制下一次调度时机

这意味着：

- 当前实现更简单，适合第一版桌面工具
- 后续如果要追求更高频率或更稳定的时序，可再拆分为独立引擎线程

### 9.2 `EngineRunner` 职责

`EngineRunner` 当前负责：

- `arm(config)`：装载并校验配置
- `start()`：进入运行状态
- `tick()`：执行一次输入动作并更新计数
- `stop()`：停止运行并回到空闲状态
- 保存累计执行次数和启动时间

### 9.3 状态机

```rust
pub enum EngineState {
    Idle,
    Armed,
    Running,
    Stopping,
    Error(String),
}
```

当前主要流转：

- `Idle -> Armed`：配置装载成功
- `Armed -> Running`：启动成功
- `Running -> Idle`：停止、完成或异常后回收

### 9.4 调度规则

当前调度依据：

- 间隔：`interval_ms`
- 完成计数：`completed_actions`
- 运行时长：`started_at.elapsed()`
- 退出判断：`engine::scheduler::should_stop`

抖动实现：

- `engine::scheduler::next_interval_ms` 已提供基础抖动计算
- 当前 GUI 已暴露 `jitter_ms`
- 后续可以继续增强为真正随机或更自然的分布模型

## 10. 输入后端

### 10.1 当前能力

当前通过 Windows `SendInput` 发送：

- 鼠标按下 / 抬起
- 键盘按下 / 抬起

抽象入口：

```rust
pub trait InputBackend {
    fn send_action(
        &mut self,
        action: &InputAction,
        press_duration_ms: u64,
    ) -> Result<(), AppError>;
}
```

### 10.2 兼容性边界

- 管理员权限程序通常要求 OxyClick 也以管理员权限运行
- 某些独占全屏程序未必接受注入输入
- 反作弊、驱动级拦截、内核保护场景不承诺兼容

## 11. 配置校验规则

当前校验逻辑位于 `src/core/validate.rs`，包括：

- 配置名称不能为空
- `interval_ms >= 1`
- `press_duration_ms <= interval_ms`
- `RunMode::Count.total > 0`
- `RunMode::Timed.duration_ms > 0`
- 开始热键与停止热键不能相同
- `jitter_ms <= interval_ms`

热键层还会额外校验：

- 紧急停止热键不能与开始、停止热键重复
- 热键字符串中的按键名必须可解析
- 热键必须至少包含一个常规键，且只能包含一个常规键

## 12. 测试与诊断

### 12.1 当前单元测试覆盖

- 配置序列化与反序列化
- 旧配置迁移
- 参数校验
- 调度器停止条件
- 引擎基础状态流转

### 12.2 常用命令

```powershell
cargo test
cargo run
cargo run -- --headless-summary
cargo build --release
```

### 12.3 开发调试入口

当前额外保留了一个仅供开发排查布局问题的参数：

```powershell
cargo run --release -- --layout-debug
```

它会输出双卡片布局相关的高度测量值，方便排查等高与补齐逻辑。

当前排查文档与实现时，优先关注这些交互事实：

- 表单修改默认只写内存，不自动写磁盘。
- 保存反馈现在主要通过按钮自身动画确认，不再依赖常驻通知栏。
- 启动前延迟属于界面启动保护机制，不是调度器的一部分。

## 13. 已知边界

- 当前仅支持 Windows
- 当前执行调度仍由 UI 循环驱动，不是独立线程
- 当前没有完整日志系统或历史记录面板
- 当前多配置结构已存在，但 GUI 尚未提供完整预设切换管理

## 14. 后续演进建议

### 14.1 功能层

- 多预设管理界面
- 导入导出配置
- 更丰富的输入动作序列
- Burst 模式与更自然的抖动
- 管理员权限与热键冲突的前置提示

### 14.2 技术层

- 将执行调度拆到独立线程
- 补充更完整的日志系统
- 增强热键与权限异常提示
- 增加更多 UI 自动化与集成测试

## 15. 总结

当前版本的 OxyClick 已经不是纯骨架，而是一个可运行、可保存、可自定义热键、可发送真实输入事件的 Windows 桌面工具。

接下来最值得继续投入的方向有三类：

- 把“预留项”做实，例如日志与诊断
- 把“当前可用”做稳，例如时序调度、热键兼容、错误提示
- 把“输入能力”做深，例如动作序列、导入导出、更多诊断信息
