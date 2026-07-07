use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    Add { window_id: u64 },
    Remove { window_id: u64 },
    List,
    ToggleActive,
    ToggleAppid { appid: String },
    ToggleTitle { title: String },
    StageList,
    Stage { window_id: u64 },
    Unstage { window_id: u64 },
    StageToggleActive,
    StageToggleAppid { appid: String },
    StageToggleTitle { title: String },
    StageAll,
    UnstageAll,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Success { message: String },
    Error { message: String },
    Data { data: String },
}
