use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::protocol;

#[derive(Parser, Debug)]
#[command(name = "nsticky")]
#[command(version)]
#[command(about = "Manage sticky windows via CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

/// Manage sticky windows across all workspaces.
#[derive(Subcommand, Debug)]
enum Commands {
    Sticky {
        #[command(subcommand)]
        action: StickyAction,
    },
    /// Manage staged windows (temporarily hidden in stage workspace).
    Stage {
        #[command(subcommand)]
        action: StageAction,
    },
}

/// Actions for sticky windows.
#[derive(Subcommand, Debug)]
enum StickyAction {
    /// Add a window to sticky list by window ID.
    #[command(alias = "a")]
    Add { window_id: u64 },
    /// Remove a window from sticky list by window ID.
    #[command(alias = "r")]
    Remove { window_id: u64 },
    /// List all sticky windows.
    #[command(alias = "l")]
    List,
    /// Toggle sticky state of the currently active window.
    #[command(alias = "t")]
    ToggleActive,
    /// Toggle sticky state of windows matching the given app ID.
    #[command(alias = "ta")]
    ToggleAppid { appid: String },
    /// Toggle sticky state of windows matching the given title.
    #[command(alias = "tt")]
    ToggleTitle { title: String },
}

/// Actions for staged windows.
#[derive(Subcommand, Debug)]
enum StageAction {
    /// List all staged windows.
    #[command(alias = "l")]
    List,
    /// Stage a window by ID (move to stage workspace).
    #[command(alias = "a")]
    Add { window_id: u64 },
    /// Unstage a window by ID (move back from stage workspace).
    #[command(alias = "r")]
    Remove { window_id: u64 },
    /// Toggle stage state of the currently active window.
    #[command(alias = "t")]
    ToggleActive,
    /// Toggle stage state of windows matching the given app ID.
    #[command(alias = "ta")]
    ToggleAppid { appid: String },
    /// Toggle stage state of windows matching the given title.
    #[command(alias = "tt")]
    ToggleTitle { title: String },
    /// Stage all sticky windows.
    #[command(alias = "aa")]
    AddAll,
    /// Unstage all staged windows.
    #[command(alias = "ra")]
    RemoveAll,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum CliResponse {
    Success { message: String },
    Error { message: String },
    Data { data: String },
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    let socket_path = "/tmp/niri_sticky_cli.sock";
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let request = match cli.command {
        Commands::Sticky { action } => match action {
            StickyAction::Add { window_id } => protocol::Request::Add { window_id },
            StickyAction::Remove { window_id } => protocol::Request::Remove { window_id },
            StickyAction::List => protocol::Request::List,
            StickyAction::ToggleActive => protocol::Request::ToggleActive,
            StickyAction::ToggleAppid { appid } => protocol::Request::ToggleAppid { appid },
            StickyAction::ToggleTitle { title } => protocol::Request::ToggleTitle { title },
        },
        Commands::Stage { action } => match action {
            StageAction::List => protocol::Request::Stage(crate::protocol::StageArgs {
                window_id: None,
                all: false,
                list: true,
                active: false,
                appid: None,
                title: None,
            }),
            StageAction::Add { window_id } => {
                protocol::Request::Stage(crate::protocol::StageArgs {
                    window_id: Some(window_id),
                    all: false,
                    list: false,
                    active: false,
                    appid: None,
                    title: None,
                })
            }
            StageAction::Remove { window_id } => {
                protocol::Request::Unstage(crate::protocol::UnstageArgs {
                    window_id: Some(window_id),
                    all: false,
                    active: false,
                })
            }
            StageAction::ToggleActive => protocol::Request::Stage(crate::protocol::StageArgs {
                window_id: None,
                all: false,
                list: false,
                active: true,
                appid: None,
                title: None,
            }),
            StageAction::ToggleAppid { appid } => {
                protocol::Request::Stage(crate::protocol::StageArgs {
                    window_id: None,
                    all: false,
                    list: false,
                    active: false,
                    appid: Some(appid),
                    title: None,
                })
            }
            StageAction::ToggleTitle { title } => {
                protocol::Request::Stage(crate::protocol::StageArgs {
                    window_id: None,
                    all: false,
                    list: false,
                    active: false,
                    appid: None,
                    title: Some(title),
                })
            }
            StageAction::AddAll => protocol::Request::Stage(crate::protocol::StageArgs {
                window_id: None,
                all: true,
                list: false,
                active: false,
                appid: None,
                title: None,
            }),
            StageAction::RemoveAll => protocol::Request::Unstage(crate::protocol::UnstageArgs {
                window_id: None,
                all: true,
                active: false,
            }),
        },
    };

    let request_json = serde_json::to_string(&request)?;
    writer.write_all(request_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut response = String::new();
    reader.read_line(&mut response).await?;
    let response = response.trim();

    let parsed: CliResponse =
        serde_json::from_str(response).context("Invalid JSON response from daemon")?;
    match parsed {
        CliResponse::Success { message } => println!("{message}"),
        CliResponse::Error { message } => {
            eprintln!("Error: {message}");
        }
        CliResponse::Data { data } => println!("{data}"),
    }

    Ok(())
}
