use std::error::Error;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    HotkeyRegisterFailed(String),
    HookInstallFailed(String),
    InputSendFailed(String),
    InvalidConfig(String),
    Io(String),
    Unsupported(String),
}

impl Display for AppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::HotkeyRegisterFailed(message) => write!(f, "热键注册失败：{message}"),
            Self::HookInstallFailed(message) => write!(f, "键盘钩子安装失败：{message}"),
            Self::InputSendFailed(message) => write!(f, "输入发送失败：{message}"),
            Self::InvalidConfig(message) => write!(f, "配置无效：{message}"),
            Self::Io(message) => write!(f, "IO 错误：{message}"),
            Self::Unsupported(message) => write!(f, "不支持的操作：{message}"),
        }
    }
}

impl Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}
