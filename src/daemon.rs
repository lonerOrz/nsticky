use anyhow::Result;
use serde_json::Value;
use std::future;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::{business::BusinessLogic, config, protocol};

pub async fn start() -> Result<()> {
    let business_logic = BusinessLogic::new();

    let cli_business_logic = business_logic.clone();
    tokio::spawn(async move {
        if let Err(e) = run_cli_server(cli_business_logic).await {
            tracing::error!("CLI server error: {e:?}");
        }
    });

    let watcher_business_logic = business_logic.clone();
    tokio::spawn(async move {
        if let Err(e) = run_watcher(watcher_business_logic).await {
            tracing::error!("Watcher error: {e:?}");
        }
    });

    tracing::info!("nsticky daemon started.");
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
            if let Err(e) = handle_cli_connection(stream, business_logic_clone).await {
                tracing::error!("CLI connection error: {e:?}");
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

    let request = match protocol::parse_request(line) {
        Ok(req) => req,
        Err(e) => {
            let response = protocol::Response::Error {
                message: e.to_string(),
            };
            let response_str = protocol::format_response(response)?;
            writer.write_all(response_str.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            return Ok(());
        }
    };

    let response = match request {
        protocol::Request::Add { window_id } => {
            match business_logic.add_sticky_window(window_id).await {
                Ok(is_new) => {
                    if is_new {
                        protocol::Response::Success {
                            message: "Added".to_string(),
                        }
                    } else {
                        protocol::Response::Success {
                            message: "Already in sticky list".to_string(),
                        }
                    }
                }
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        }
        protocol::Request::Remove { window_id } => {
            match business_logic.remove_sticky_window(window_id).await {
                Ok(was_present) => {
                    if was_present {
                        protocol::Response::Success {
                            message: "Removed".to_string(),
                        }
                    } else {
                        protocol::Response::Success {
                            message: "Not in sticky list".to_string(),
                        }
                    }
                }
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        }
        protocol::Request::List => match business_logic.list_sticky_windows().await {
            Ok(windows) => protocol::Response::Data {
                data: format!("{windows:?}"),
            },
            Err(e) => protocol::Response::Error {
                message: e.to_string(),
            },
        },
        protocol::Request::ToggleActive => match business_logic.toggle_active_window().await {
            Ok(was_added) => {
                if was_added {
                    protocol::Response::Success {
                        message: "Added active window to sticky".to_string(),
                    }
                } else {
                    protocol::Response::Success {
                        message: "Removed active window from sticky".to_string(),
                    }
                }
            }
            Err(e) => protocol::Response::Error {
                message: e.to_string(),
            },
        },
        protocol::Request::ToggleAppid { appid } => {
            match business_logic.toggle_by_appid(&appid).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        }
        protocol::Request::ToggleTitle { title } => {
            match business_logic.toggle_by_title(&title).await {
                Ok(count) => protocol::Response::Success {
                    message: format!("Toggled {count} window(s)"),
                },
                Err(e) => protocol::Response::Error {
                    message: e.to_string(),
                },
            }
        }
        protocol::Request::Stage(stage_args) => {
            if stage_args.active {
                let active_id = match crate::system_integration::get_active_window_id().await {
                    Ok(id) => id,
                    Err(_) => {
                        let response = protocol::Response::Error {
                            message: "Failed to get active window".to_string(),
                        };
                        let response_str = protocol::format_response(response)?;
                        return Ok(writer.write_all(response_str.as_bytes()).await?);
                    }
                };

                let is_staged = business_logic.is_window_staged(active_id).await;
                if is_staged {
                    let current_ws_id =
                        match crate::system_integration::get_active_workspace_id().await {
                            Ok(id) => id,
                            Err(_) => {
                                let response = protocol::Response::Error {
                                    message: "Failed to get active workspace ID".to_string(),
                                };
                                let response_str = protocol::format_response(response)?;
                                return Ok(writer.write_all(response_str.as_bytes()).await?);
                            }
                        };
                    match business_logic.unstage_active_window(current_ws_id).await {
                        Ok(()) => protocol::Response::Success {
                            message: "Unstaged active window".to_string(),
                        },
                        Err(e) => protocol::Response::Error {
                            message: e.to_string(),
                        },
                    }
                } else {
                    match business_logic.stage_active_window().await {
                        Ok(()) => protocol::Response::Success {
                            message: "Staged active window".to_string(),
                        },
                        Err(e) => protocol::Response::Error {
                            message: e.to_string(),
                        },
                    }
                }
            } else if let Some(appid) = stage_args.appid {
                let current_ws_id = match crate::system_integration::get_active_workspace_id().await
                {
                    Ok(id) => id,
                    Err(_) => {
                        let response = protocol::Response::Error {
                            message: "Failed to get active workspace ID".to_string(),
                        };
                        let response_str = protocol::format_response(response)?;
                        return Ok(writer.write_all(response_str.as_bytes()).await?);
                    }
                };
                match business_logic
                    .toggle_stage_by_appid(&appid, current_ws_id)
                    .await
                {
                    Ok(count) => protocol::Response::Success {
                        message: format!("Toggled {count} window(s)"),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if let Some(title) = stage_args.title {
                let current_ws_id = match crate::system_integration::get_active_workspace_id().await
                {
                    Ok(id) => id,
                    Err(_) => {
                        let response = protocol::Response::Error {
                            message: "Failed to get active workspace ID".to_string(),
                        };
                        let response_str = protocol::format_response(response)?;
                        return Ok(writer.write_all(response_str.as_bytes()).await?);
                    }
                };
                match business_logic
                    .toggle_stage_by_title(&title, current_ws_id)
                    .await
                {
                    Ok(count) => protocol::Response::Success {
                        message: format!("Toggled {count} window(s)"),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if stage_args.all {
                match business_logic.stage_all_windows().await {
                    Ok(count) => protocol::Response::Success {
                        message: format!("Staged {count} windows"),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if stage_args.list {
                match business_logic.list_staged_windows().await {
                    Ok(windows) => protocol::Response::Data {
                        data: format!("{windows:?}"),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if let Some(window_id) = stage_args.window_id {
                match business_logic.stage_window(window_id).await {
                    Ok(()) => protocol::Response::Success {
                        message: "Staged window".to_string(),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else {
                protocol::Response::Error {
                    message: "Invalid stage command".to_string(),
                }
            }
        }
        protocol::Request::Unstage(unstage_args) => {
            let current_ws_id = match crate::system_integration::get_active_workspace_id().await {
                Ok(id) => id,
                Err(_) => {
                    let response = protocol::Response::Error {
                        message: "Failed to get active workspace ID".to_string(),
                    };
                    let response_str = protocol::format_response(response)?;
                    return Ok(writer.write_all(response_str.as_bytes()).await?);
                }
            };

            if unstage_args.all {
                match business_logic.unstage_all_windows(current_ws_id).await {
                    Ok(count) => protocol::Response::Success {
                        message: format!("Unstaged {count} windows"),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if unstage_args.active {
                match business_logic.unstage_active_window(current_ws_id).await {
                    Ok(()) => protocol::Response::Success {
                        message: "Unstaged active window".to_string(),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else if let Some(window_id) = unstage_args.window_id {
                match business_logic
                    .unstage_window(window_id, current_ws_id)
                    .await
                {
                    Ok(()) => protocol::Response::Success {
                        message: "Unstaged window".to_string(),
                    },
                    Err(e) => protocol::Response::Error {
                        message: e.to_string(),
                    },
                }
            } else {
                protocol::Response::Error {
                    message: "Invalid unstage command".to_string(),
                }
            }
        }
    };

    let response_str = protocol::format_response(response)?;
    writer.write_all(response_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;

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
                tracing::info!("Workspace switched to: {ws_id}");
                if let Err(e) = business_logic.handle_workspace_activation(ws_id).await {
                    tracing::error!("Failed to handle workspace activation: {e:?}");
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
                    let is_in_staged = business_logic.is_window_staged(id).await;
                    let is_in_sticky = business_logic.is_window_sticky(id).await;
                    let was_auto_sticky = auto_staged_windows.contains(&id);

                    if was_auto_sticky && (!is_in_sticky || is_in_staged) {
                        continue;
                    }

                    if config.match_sticky(&app_id, &title) {
                        auto_staged_windows.insert(id);
                        tracing::info!("Auto-sticky window {id} ({app_id:?})");
                        if let Err(e) = business_logic.add_sticky_window(id).await {
                            tracing::error!("Failed to auto-sticky window {id}: {e:?}");
                        }
                    }
                }
            }

            if let Some(closed_event) = v.get("WindowClosed")
                && let Some(window_id) = closed_event.get("id").and_then(|id| id.as_u64())
            {
                tracing::info!("Window closed: {window_id}");
                auto_staged_windows.remove(&window_id);
                let _ = business_logic
                    .remove_window_unconditionally(window_id)
                    .await;
            }
        }
        line.clear();
    }

    Ok(())
}
