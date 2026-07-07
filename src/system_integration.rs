use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::HashSet;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    time::{Duration, timeout},
};

const IPC_TIMEOUT: Duration = Duration::from_secs(5);

pub enum WorkspaceRef<'a> {
    Id(u64),
    Name(&'a str),
}

#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

async fn send_ipc_request(request: &Value) -> Result<Value> {
    timeout(IPC_TIMEOUT, async {
        let socket_path = std::env::var("NIRI_SOCKET")
            .context("NIRI_SOCKET env var not set")?;

        let stream = UnixStream::connect(&socket_path).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let cmd_str = serde_json::to_string(request)? + "\n";
        writer.write_all(cmd_str.as_bytes()).await?;
        writer.flush().await?;

        let mut response = String::new();
        reader.read_line(&mut response).await?;
        let response_json: Value = serde_json::from_str(response.trim())?;

        if let Some(err) = response_json.get("Err") {
            anyhow::bail!("Niri IPC error: {}", err);
        }

        if let Some(ok_payload) = response_json.get("Ok") {
            return Ok(ok_payload.clone());
        }

        anyhow::bail!("Unexpected Niri reply format: {}", response);
    })
    .await
    .context("Niri IPC timeout")?
}

pub async fn get_active_workspace_id() -> Result<u64> {
    let ok_val = send_ipc_request(&json!("Workspaces")).await?;
    let workspaces = ok_val
        .get("Workspaces")
        .and_then(|w| w.as_array())
        .context("Workspaces field not found or not an array")?;

    for workspace in workspaces {
        if workspace.get("is_active").and_then(|v| v.as_bool()) == Some(true)
            && let Some(id) = workspace.get("id").and_then(|v| v.as_u64())
        {
            return Ok(id);
        }
    }

    anyhow::bail!("Active workspace not found");
}

pub async fn get_active_window_id() -> Result<u64> {
    let ok_val = send_ipc_request(&json!("FocusedWindow")).await?;
    let focused_window = ok_val
        .get("FocusedWindow")
        .context("FocusedWindow field not found in reply")?;

    if let Some(id) = focused_window.get("id").and_then(|v| v.as_u64()) {
        Ok(id)
    } else {
        anyhow::bail!("Focused window id not found");
    }
}

async fn get_full_window_info() -> Result<Vec<WindowInfo>> {
    let ok_val = send_ipc_request(&json!("Windows")).await?;
    let windows_arr = ok_val
        .get("Windows")
        .and_then(|w| w.as_array())
        .context("Windows field not found or not an array")?;

    let mut windows = Vec::new();
    for item in windows_arr {
        if let Some(id) = item.get("id").and_then(|v| v.as_u64()) {
            let app_id = item
                .get("app_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            windows.push(WindowInfo { id, app_id, title });
        }
    }
    Ok(windows)
}

pub async fn get_full_window_list() -> Result<HashSet<u64>> {
    let windows = get_full_window_info().await?;
    Ok(windows.into_iter().map(|w| w.id).collect())
}

pub async fn find_windows_by_appid(appid: &str) -> Result<Vec<u64>> {
    let windows = get_full_window_info().await?;
    let mut ids = Vec::new();
    for window in windows {
        if let Some(window_appid) = window.app_id
            && window_appid == appid
        {
            ids.push(window.id);
        }
    }
    Ok(ids)
}

pub async fn find_windows_by_title(title: &str) -> Result<Vec<u64>> {
    let windows = get_full_window_info().await?;
    let mut ids = Vec::new();
    for window in windows {
        if let Some(window_title) = window.title
            && window_title.contains(title)
        {
            ids.push(window.id);
        }
    }
    Ok(ids)
}

pub async fn move_to_workspace(win_id: u64, dest: WorkspaceRef<'_>) -> Result<()> {
    let reference = match dest {
        WorkspaceRef::Id(id) => json!({ "Id": id }),
        WorkspaceRef::Name(name) => json!({ "Name": name }),
    };

    let cmd = json!({
        "Action": {
            "MoveWindowToWorkspace": {
                "window_id": win_id,
                "focus": false,
                "reference": reference
            }
        }
    });

    let _ = send_ipc_request(&cmd).await?;
    Ok(())
}
