use anyhow::{Context, Result};
use std::future;

use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
};

use crate::{business::BusinessLogic, protocol};

pub async fn start() -> Result<()> {
    let config = crate::config::Config::load_or_default();
    let business_logic = BusinessLogic::new(config);

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

    let response = match serde_json::from_str::<protocol::Request>(line) {
        Ok(request) => business_logic.handle_request(request).await,
        Err(e) => protocol::Response::Error {
            message: format!("Failed to parse request: {e}"),
        },
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
