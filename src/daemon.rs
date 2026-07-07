use anyhow::{Context, Result};
use std::future;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::{business::BusinessLogic, protocol};

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

async fn send_error(writer: &mut (impl AsyncWriteExt + Unpin), msg: &str) -> Result<()> {
    let response = protocol::Response::Error {
        message: msg.to_string(),
    };
    let response_str = serde_json::to_string(&response).context("Failed to serialize response")?;
    writer.write_all(response_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    Ok(())
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

    let request = match serde_json::from_str(line).context("Failed to parse request") {
        Ok(req) => req,
        Err(e) => {
            let response = protocol::Response::Error {
                message: e.to_string(),
            };
            let response_str =
                serde_json::to_string(&response).context("Failed to serialize response")?;
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
                        send_error(&mut writer, "Failed to get active window").await?;
                        return Ok(());
                    }
                };

                let is_staged = business_logic.is_window_staged(active_id).await;
                if is_staged {
                    let current_ws_id = match crate::system_integration::get_active_workspace_id()
                        .await
                    {
                        Ok(id) => id,
                        Err(_) => {
                            send_error(&mut writer, "Failed to get active workspace ID").await?;
                            return Ok(());
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
                        send_error(&mut writer, "Failed to get active workspace ID").await?;
                        return Ok(());
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
                        send_error(&mut writer, "Failed to get active workspace ID").await?;
                        return Ok(());
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
                    send_error(&mut writer, "Failed to get active workspace ID").await?;
                    return Ok(());
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

    let response_str = serde_json::to_string(&response).context("Failed to serialize response")?;
    writer.write_all(response_str.as_bytes()).await?;
    writer.write_all(b"\n").await?;

    Ok(())
}

async fn run_watcher(business_logic: BusinessLogic) -> Result<()> {
    let mut event_stream = crate::system_integration::get_event_stream().await?;

    use crate::system_integration::NiriEvent;
    while let Some(event) = event_stream.next_event().await? {
        match event {
            NiriEvent::WorkspaceActivated { id: ws_id } => {
                tracing::info!("Workspace switched to: {ws_id}");
                if let Err(e) = business_logic.handle_workspace_activation(ws_id).await {
                    tracing::error!("Failed to handle workspace activation: {e:?}");
                }
            }
            NiriEvent::WindowOpenedOrChanged { id, app_id, title } => {
                if let Err(e) = business_logic
                    .handle_window_opened_or_changed(id, app_id, title)
                    .await
                {
                    tracing::error!("Failed to handle window opened or changed: {e:?}");
                }
            }
            NiriEvent::WindowClosed { id: window_id } => {
                tracing::info!("Window closed: {window_id}");
                if let Err(e) = business_logic.handle_window_closed(window_id).await {
                    tracing::error!("Failed to handle window closed: {e:?}");
                }
            }
        }
    }

    Ok(())
}
