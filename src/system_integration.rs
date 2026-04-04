use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::HashSet;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
    process::Command,
    time::{Duration, timeout},
};

const IPC_TIMEOUT: Duration = Duration::from_secs(5);

/// Window information structure
#[derive(Debug, Clone)]
pub struct WindowInfo {
    pub id: u64,
    pub app_id: Option<String>,
    pub title: Option<String>,
}

/// Get active workspace ID from Niri
pub async fn get_active_workspace_id() -> Result<u64> {
    let output = timeout(
        IPC_TIMEOUT,
        tokio::process::Command::new("niri")
            .args(["msg", "-j", "workspaces"])
            .output(),
    )
    .await
    .context("IPC timeout getting workspaces")??;

    if !output.status.success() {
        anyhow::bail!("Failed to get workspaces");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)?;

    if let Some(workspaces) = json.as_array() {
        for workspace in workspaces {
            if workspace.get("is_active").and_then(|v| v.as_bool()) == Some(true)
                && let Some(id) = workspace.get("id").and_then(|v| v.as_u64())
            {
                return Ok(id);
            }
        }
    }

    anyhow::bail!("Active workspace not found");
}

/// Get active window ID from Niri
pub async fn get_active_window_id() -> Result<u64> {
    let output = timeout(
        IPC_TIMEOUT,
        tokio::process::Command::new("niri")
            .args(["msg", "--json", "focused-window"])
            .output(),
    )
    .await
    .context("IPC timeout getting focused window")??;
    if !output.status.success() {
        anyhow::bail!("Failed to get focused window");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)?;
    if let Some(id) = json.get("id").and_then(|v| v.as_u64()) {
        Ok(id)
    } else {
        anyhow::bail!("Focused window id not found");
    }
}

/// Get full window information from Niri
async fn get_full_window_info() -> Result<Vec<WindowInfo>> {
    let output = timeout(
        IPC_TIMEOUT,
        Command::new("niri")
            .args(["msg", "--json", "windows"])
            .output(),
    )
    .await
    .context("IPC timeout getting windows list")??;
    if !output.status.success() {
        anyhow::bail!("Failed to get windows list");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout)?;
    let mut windows = Vec::new();
    if let Some(arr) = json.as_array() {
        for item in arr {
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
    }
    Ok(windows)
}

/// Get full window list from Niri
pub async fn get_full_window_list() -> Result<HashSet<u64>> {
    let windows = get_full_window_info().await?;
    Ok(windows.into_iter().map(|w| w.id).collect())
}

/// Find window by application ID (returns first match)
#[allow(dead_code)]
pub async fn find_window_by_appid(appid: &str) -> Result<Option<u64>> {
    let windows = get_full_window_info().await?;
    for window in windows {
        if let Some(window_appid) = window.app_id
            && window_appid == appid
        {
            return Ok(Some(window.id));
        }
    }
    Ok(None)
}

/// Find all windows by application ID (returns all matches)
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

/// Find window by title (returns first match)
#[allow(dead_code)]
pub async fn find_window_by_title(title: &str) -> Result<Option<u64>> {
    let windows = get_full_window_info().await?;
    for window in windows {
        if let Some(window_title) = window.title
            && window_title.contains(title)
        {
            return Ok(Some(window.id));
        }
    }
    Ok(None)
}

/// Find all windows by title (returns all matches)
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

/// Move window to workspace
pub async fn move_to_workspace(win_id: u64, ws_id: u64) -> Result<()> {
    timeout(IPC_TIMEOUT, async move {
        let socket_path = std::env::var("NIRI_SOCKET")?;

        let stream = UnixStream::connect(&socket_path).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        let cmd = json!({
            "Action": {
                "MoveWindowToWorkspace": {
                    "window_id": win_id,
                    "focus": false,
                    "reference": { "Id": ws_id }
                }
            }
        });
        let cmd_str = serde_json::to_string(&cmd)? + "\n";

        writer.write_all(cmd_str.as_bytes()).await?;
        writer.flush().await?;

        let mut response = String::new();
        reader.read_line(&mut response).await?;
        let response_json: Value = serde_json::from_str(response.trim())?;

        if let Some(err_msg) = response_json.get("Err").and_then(|v| v.as_str()) {
            anyhow::bail!("Move to workspace failed: {}", err_msg);
        }

        if response_json.get("Ok").is_some() {
            return Ok(());
        }

        anyhow::bail!("Unexpected response format: {}", response);
    })
    .await
    .context("IPC timeout moving window to workspace")?
}

/// Move window to named workspace
pub async fn move_to_named_workspace(win_id: u64, workspace_name: &str) -> Result<()> {
    timeout(IPC_TIMEOUT, async move {
        let socket_path = std::env::var("NIRI_SOCKET")?;
        let stream = UnixStream::connect(&socket_path).await?;
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        let cmd = json!({
            "Action": {
                "MoveWindowToWorkspace": {
                    "window_id": win_id,
                    "focus": false,
                    "reference": { "Name": workspace_name }
                }
            }
        });
        let cmd_str = serde_json::to_string(&cmd)? + "\n";
        writer.write_all(cmd_str.as_bytes()).await?;
        writer.flush().await?;
        let mut response = String::new();
        reader.read_line(&mut response).await?;
        let response_json: Value = serde_json::from_str(response.trim())?;

        if let Some(err_msg) = response_json.get("Err").and_then(|v| v.as_str()) {
            anyhow::bail!("Move to named workspace failed: {}", err_msg);
        }

        if response_json.get("Ok").is_some() {
            return Ok(());
        }

        anyhow::bail!("Unexpected response format: {}", response);
    })
    .await
    .context("IPC timeout moving window to named workspace")?
}
