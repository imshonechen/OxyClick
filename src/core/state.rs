use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineState {
    Idle,
    Armed,
    Running,
    Stopping,
    Error(String),
}

impl Display for EngineState {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Idle => write!(f, "空闲"),
            Self::Armed => write!(f, "已装载"),
            Self::Running => write!(f, "运行中"),
            Self::Stopping => write!(f, "停止中"),
            Self::Error(message) => write!(f, "错误（{message}）"),
        }
    }
}
