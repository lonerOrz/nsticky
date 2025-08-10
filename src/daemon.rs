use anyhow::Result;
use serde_json::{Value, json};
use std::future;
use std::{collections::HashSet, env, sync::Arc};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    process::Command,
    sync::Mutex,
};

pub async fn start(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Result<()> {
    let sticky_clone = sticky_windows.clone();
    tokio::spawn(async move {
        if let Err(e) = run_cli_server(sticky_clone).await {
            eprintln!("CLI server error: {:?}", e);
        }
    });

    let sticky_clone = sticky_windows.clone();
    tokio::spawn(async move {
        if let Err(e) = run_watcher(sticky_clone).await {
            eprintln!("Watcher error: {:?}", e);
        }
    });

    println!("nsticky daemon started.");

    future::pending::<()>().await;
    Ok(())
}

async fn run_cli_server(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Result<()> {
    let cli_socket_path = "/tmp/niri_sticky_cli.sock";
    let _ = std::fs::remove_file(cli_socket_path);
    let listener = UnixListener::bind(cli_socket_path)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let sticky_clone = sticky_windows.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_cli_connection(stream, sticky_clone).await {
                eprintln!("CLI connection error: {:?}", e);
            }
        });
    }
}

pub async fn handle_cli_connection(
    stream: UnixStream,
    sticky_windows: Arc<Mutex<HashSet<u64>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }
    let line = line.trim();
    let mut parts = line.split_whitespace();

    match parts.next() {
        Some("add") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    // 锁外检查窗口是否存在
                    let full_window_list = get_full_window_list().await?;
                    if !full_window_list.contains(&id) {
                        writer.write_all(b"Window not found in Niri\n").await?;
                        return Ok(());
                    }

                    // 锁内添加
                    let mut sticky = sticky_windows.lock().await;
                    if sticky.insert(id) {
                        writer.write_all(b"Added\n").await?;
                    } else {
                        writer.write_all(b"Already in sticky list\n").await?;
                    }
                } else {
                    writer.write_all(b"Invalid window id\n").await?;
                }
            } else {
                writer.write_all(b"Missing window id\n").await?;
            }
        }

        Some("remove") => {
            if let Some(id_str) = parts.next() {
                if let Ok(id) = id_str.parse::<u64>() {
                    // 锁外检查窗口是否存在
                    let full_window_list = get_full_window_list().await?;
                    if !full_window_list.contains(&id) {
                        writer.write_all(b"Window not found in Niri\n").await?;
                        return Ok(());
                    }

                    // 锁内删除
                    let mut sticky = sticky_windows.lock().await;
                    if sticky.remove(&id) {
                        writer.write_all(b"Removed\n").await?;
                    } else {
                        writer.write_all(b"Not in sticky list\n").await?;
                    }
                } else {
                    writer.write_all(b"Invalid window id\n").await?;
                }
            } else {
                writer.write_all(b"Missing window id\n").await?;
            }
        }

        Some("list") => {
            // 拿锁复制快照
            let snapshot: Vec<u64> = {
                let sticky = sticky_windows.lock().await;
                sticky.iter().copied().collect()
            };

            // 锁外查询 niri 当前存在的窗口
            let full_window_list = get_full_window_list().await?;
            let valid: Vec<u64> = snapshot
                .into_iter()
                .filter(|id| full_window_list.contains(id))
                .collect();

            let list_str = format!("{:?}\n", valid);
            writer.write_all(list_str.as_bytes()).await?;
        }

        Some("toggle_active") => {
            // 获取当前活动窗口ID
            let active_id = match get_active_window_id().await {
                Ok(id) => id,
                Err(_) => {
                    writer.write_all(b"Failed to get active window\n").await?;
                    return Ok(());
                }
            };

            // 锁外检查窗口是否存在
            let full_window_list = get_full_window_list().await?;
            if !full_window_list.contains(&active_id) {
                writer
                    .write_all(b"Active window not found in Niri\n")
                    .await?;
                return Ok(());
            }

            // 锁内操作 toggle
            let mut sticky = sticky_windows.lock().await;
            if sticky.contains(&active_id) {
                sticky.remove(&active_id);
                writer
                    .write_all(b"Removed active window from sticky\n")
                    .await?;
            } else {
                sticky.insert(active_id);
                writer.write_all(b"Added active window to sticky\n").await?;
            }
        }

        _ => {
            writer.write_all(b"Unknown command\n").await?;
        }
    }

    Ok(())
}

// 获取active窗口的ID
async fn get_active_window_id() -> Result<u64> {
    let output = tokio::process::Command::new("niri")
        .args(&["msg", "--json", "focused-window"])
        .output()
        .await?;

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

async fn run_watcher(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Result<()> {
    let socket_path = env::var("NIRI_SOCKET").expect("NIRI_SOCKET env var not set");
    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer.write_all(b"\"EventStream\"\n").await?;
    writer.flush().await?;

    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if let Some(ws) = v.get("WorkspaceActivated") {
                if let Some(ws_id) = ws.get("id").and_then(|id| id.as_u64()) {
                    println!("Workspace switched to: {}", ws_id);

                    let sticky_snapshot = {
                        let mut sticky = sticky_windows.lock().await;
                        let full_window_list = get_full_window_list().await.unwrap_or_default();
                        sticky.retain(|win_id| full_window_list.contains(win_id));
                        println!("Updated sticky windows: {:?}", *sticky);
                        sticky.clone()
                    };

                    for win_id in sticky_snapshot.iter() {
                        if let Err(e) = move_to_workspace(*win_id, ws_id).await {
                            eprintln!("Failed to move window {}: {:?}", win_id, e);
                        }
                    }
                }
            }
        }
        line.clear();
    }

    Ok(())
}

async fn get_full_window_list() -> Result<HashSet<u64>> {
    let output = Command::new("niri")
        .args(&["msg", "--json", "windows"])
        .output()
        .await?;

    if !output.status.success() {
        anyhow::bail!("Failed to get windows list");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: Value = serde_json::from_str(&stdout)?;

    let mut window_ids = HashSet::new();
    if let Some(arr) = json.as_array() {
        for item in arr {
            if let Some(id) = item.get("id").and_then(|v| v.as_u64()) {
                window_ids.insert(id);
            }
        }
    }

    Ok(window_ids)
}

async fn move_to_workspace(win_id: u64, ws_id: u64) -> Result<()> {
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
    println!("move_to_workspace response: {}", response.trim());

    Ok(())
}
