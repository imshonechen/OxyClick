use crate::core::model::ClickTaskConfig;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCommand {
    Arm(ClickTaskConfig),
    Start,
    Stop,
    EmergencyStop,
    UpdateConfig(ClickTaskConfig),
}
