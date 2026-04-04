use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Define request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Request {
    Add { window_id: u64 },
    Remove { window_id: u64 },
    List,
    ToggleActive,
    ToggleAppid { appid: String },
    ToggleTitle { title: String },
    Stage(StageArgs),
    Unstage(UnstageArgs),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct StageArgs {
    pub window_id: Option<u64>,
    pub all: bool,
    pub list: bool,
    pub active: bool,
    pub appid: Option<String>,
    pub title: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct UnstageArgs {
    pub window_id: Option<u64>,
    pub all: bool,
    pub active: bool,
    pub appid: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Response {
    Success { message: String },
    Error { message: String },
    Data { data: String },
}

/// Parse JSON string to Request
pub fn parse_request(line: &str) -> Result<Request> {
    serde_json::from_str(line).context("Failed to parse request")
}

pub fn format_response(response: Response) -> Result<String> {
    serde_json::to_string(&response).context("Failed to serialize response")
}
