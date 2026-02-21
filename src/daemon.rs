use anyhow::Result;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::future;
use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    sync::Mutex,
};

use crate::{business::BusinessLogic, config, protocol};

pub async fn start(sticky_windows: Arc<Mutex<HashSet<u64>>>) -> Result<()> {
    let staged_set = Arc::new(Mutex::new(HashMap::new()));
    let business_logic = BusinessLogic::new(sticky_windows, staged_set);

    let cli_business_logic = business_logic.clone();
    tokio::spawn(async move {
        if let Err(_e) = run_cli_server(cli_business_logic).await {
            eprintln!("CLI server error: {_e:?}");
        }
    });

    let watcher_business_logic = business_logic.clone();
    tokio::spawn(async move {
        if let Err(_e) = run_watcher(watcher_business_logic).await {
            eprintln!("Watcher error: {_e:?}");
        }
    });

    println!("nsticky daemon started.");
    future::pending::<()>().await;
    Ok(())
}

async fn run_cli_server(business_logic: BusinessLogic) -> Result<()> {
    let cli_socket_path = "/tmp/niri_sticky_cli.sock";
    let _ = std::fs::remove_file(cli_socket_path);
    let listener = UnixListener::bind(cli_socket_path)?;

    loop {
        let (stream, _) = listener.accept().await?;
        let business_logic_clone = business_logic.clone();
        tokio::spawn(async move {
            if let Err(_e) = handle_cli_connection(stream, business_logic_clone).await {
                eprintln!("CLI connection error: {_e:?}");
            }
        });
    }
}

async fn handle_cli_connection(stream: UnixStream, business_logic: BusinessLogic) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    let n = reader.read_line(&mut line).await?;
    if n == 0 {
        return Ok(());
    }
    let line = line.trim();

    // Parse request
    let request = match protocol::parse_request(line) {
        Ok(req) => req,
        Err(e) => {
            writer
                .write_all(format!("Error: {}\n", e).as_bytes())
                .await?;
            return Ok(());
        }
    };

    // Process request and generate response
    let response = match request {
        protocol::Request::Add { window_id } => {
            match business_logic.add_sticky_window(window_id).await {
                Ok(is_new) => {
                    if is_new {
                        protocol::Response::Success("Added\n".to_string())
                    } else {
                        protocol::Response::Success("Already in sticky list\n".to_string())
                    }
                }
                Err(e) => protocol::Response::Error(e.to_string()),
            }
        }
        protocol::Request::Remove { window_id } => {
            match business_logic.remove_sticky_window(window_id).await {
                Ok(was_present) => {
                    if was_present {
                        protocol::Response::Success("Removed\n".to_string())
                    } else {
                        protocol::Response::Success("Not in sticky list\n".to_string())
                    }
                }
                Err(e) => protocol::Response::Error(e.to_string()),
            }
        }
        protocol::Request::List => match business_logic.list_sticky_windows().await {
            Ok(windows) => protocol::Response::Data(format!("{:?}\n", windows)),
            Err(e) => protocol::Response::Error(e.to_string()),
        },
        protocol::Request::ToggleActive => match business_logic.toggle_active_window().await {
            Ok(was_added) => {
                if was_added {
                    protocol::Response::Success("Added active window to sticky\n".to_string())
                } else {
                    protocol::Response::Success("Removed active window from sticky\n".to_string())
                }
            }
            Err(e) => protocol::Response::Error(e.to_string()),
        },
        protocol::Request::ToggleAppid { appid } => {
            match business_logic.toggle_by_appid(&appid).await {
                Ok(was_added) => {
                    if was_added {
                        protocol::Response::Success("Added window to sticky\n".to_string())
                    } else {
                        protocol::Response::Success("Removed window from sticky\n".to_string())
                    }
                }
                Err(e) => protocol::Response::Error(e.to_string()),
            }
        }
        protocol::Request::ToggleTitle { title } => {
            match business_logic.toggle_by_title(&title).await {
                Ok(was_added) => {
                    if was_added {
                        protocol::Response::Success("Added window to sticky\n".to_string())
                    } else {
                        protocol::Response::Success("Removed window from sticky\n".to_string())
                    }
                }
                Err(e) => protocol::Response::Error(e.to_string()),
            }
        }
        protocol::Request::Stage(stage_args) => {
            if stage_args.active {
                // Toggle active window stage status (unstage if staged, stage if sticky)
                let active_id = match crate::system_integration::get_active_window_id().await {
                    Ok(id) => id,
                    Err(_) => {
                        return Ok(writer.write_all(b"Failed to get active window\n").await?);
                    }
                };

                let is_staged = business_logic.is_window_staged(active_id).await;
                if is_staged {
                    let current_ws_id =
                        match crate::system_integration::get_active_workspace_id().await {
                            Ok(id) => id,
                            Err(_) => {
                                return Ok(writer
                                    .write_all(b"Failed to get active workspace ID\n")
                                    .await?);
                            }
                        };
                    match business_logic.unstage_active_window(current_ws_id).await {
                        Ok(()) => {
                            protocol::Response::Success("Unstaged active window\n".to_string())
                        }
                        Err(e) => protocol::Response::Error(e.to_string()),
                    }
                } else {
                    match business_logic.stage_active_window().await {
                        Ok(()) => protocol::Response::Success("Staged active window\n".to_string()),
                        Err(e) => protocol::Response::Error(e.to_string()),
                    }
                }
            } else if let Some(appid) = stage_args.appid {
                let current_ws_id = match crate::system_integration::get_active_workspace_id().await
                {
                    Ok(id) => id,
                    Err(_) => {
                        return Ok(writer
                            .write_all(b"Failed to get active workspace ID\n")
                            .await?);
                    }
                };
                match business_logic
                    .toggle_stage_by_appid(&appid, current_ws_id)
                    .await
                {
                    Ok(()) => {
                        protocol::Response::Success("Toggled stage status by app ID\n".to_string())
                    }
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if let Some(title) = stage_args.title {
                let current_ws_id = match crate::system_integration::get_active_workspace_id().await
                {
                    Ok(id) => id,
                    Err(_) => {
                        return Ok(writer
                            .write_all(b"Failed to get active workspace ID\n")
                            .await?);
                    }
                };
                match business_logic
                    .toggle_stage_by_title(&title, current_ws_id)
                    .await
                {
                    Ok(()) => {
                        protocol::Response::Success("Toggled stage status by title\n".to_string())
                    }
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if stage_args.all {
                match business_logic.stage_all_windows().await {
                    Ok(count) => protocol::Response::Success(format!("Staged {} windows\n", count)),
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if stage_args.list {
                match business_logic.list_staged_windows().await {
                    Ok(windows) => protocol::Response::Data(format!("{:?}\n", windows)),
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if let Some(window_id) = stage_args.window_id {
                match business_logic.stage_window(window_id).await {
                    Ok(()) => protocol::Response::Success("Staged window\n".to_string()),
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else {
                protocol::Response::Error("Invalid stage command".to_string())
            }
        }
        protocol::Request::Unstage(unstage_args) => {
            let current_ws_id = match crate::system_integration::get_active_workspace_id().await {
                Ok(id) => id,
                Err(_) => {
                    return Ok(writer
                        .write_all(b"Failed to get active workspace ID\n")
                        .await?);
                }
            };

            if unstage_args.all {
                match business_logic.unstage_all_windows(current_ws_id).await {
                    Ok(count) => {
                        protocol::Response::Success(format!("Unstaged {} windows\n", count))
                    }
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if unstage_args.active {
                match business_logic.unstage_active_window(current_ws_id).await {
                    Ok(()) => protocol::Response::Success("Unstaged active window\n".to_string()),
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else if let Some(window_id) = unstage_args.window_id {
                match business_logic
                    .unstage_window(window_id, current_ws_id)
                    .await
                {
                    Ok(()) => protocol::Response::Success("Unstaged window\n".to_string()),
                    Err(e) => protocol::Response::Error(e.to_string()),
                }
            } else {
                protocol::Response::Error("Invalid unstage command".to_string())
            }
        }
    };

    // Send response
    let response_str = protocol::format_response(response);
    writer.write_all(response_str.as_bytes()).await?;

    Ok(())
}

async fn run_watcher(business_logic: BusinessLogic) -> Result<()> {
    let socket_path = std::env::var("NIRI_SOCKET").expect("NIRI_SOCKET env var not set");
    let stream = UnixStream::connect(&socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    writer.write_all(b"\"EventStream\"\n").await?;
    writer.flush().await?;

    let mut line = String::new();

    let config = config::get_config();
    let mut auto_staged_windows: std::collections::HashSet<u64> = std::collections::HashSet::new();

    while reader.read_line(&mut line).await? > 0 {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            if let Some(ws) = v.get("WorkspaceActivated")
                && let Some(ws_id) = ws.get("id").and_then(|id| id.as_u64())
            {
                println!("Workspace switched to: {ws_id}");
                if let Err(_e) = business_logic.handle_workspace_activation(ws_id).await {
                    eprintln!("Failed to handle workspace activation: {_e:?}");
                }
            }

            if let Some(window_event) = v.get("WindowOpenedOrChanged")
                && let Some(window) = window_event.get("window")
            {
                let app_id = window
                    .get("app_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let title = window
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let window_id = window.get("id").and_then(|v| v.as_u64());

                if let Some(id) = window_id {
                    // Skip if already auto-sticky'd AND currently in staged
                    // (but allow if it's back in sticky - user may have unstaged it)
                    let is_in_staged = business_logic.is_window_staged(id).await;
                    let was_auto_sticky = auto_staged_windows.contains(&id);

                    if is_in_staged && was_auto_sticky {
                        continue;
                    }

                    if config.match_sticky(&app_id, &title) {
                        auto_staged_windows.insert(id);
                        println!("Auto-sticky window {} ({:?})", id, app_id);
                        if let Err(e) = business_logic.add_sticky_window(id).await {
                            eprintln!("Failed to auto-sticky window {}: {:?}", id, e);
                        }
                    }
                }
            }
        }
        line.clear();
    }

    Ok(())
}
