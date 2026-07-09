use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::Write;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::UnixStream,
};

use crate::protocol;
use crate::system_integration::WindowInfo;

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
    /// List open windows with optional filters.
    Windows {
        /// Filter by application ID (partial match).
        #[arg(long)]
        app_id: Option<String>,
        /// Filter by window title (partial match).
        #[arg(long)]
        title: Option<String>,
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
    /// Interactive restore: list staged windows with names and pick one to unstage.
    #[command(alias = "rs")]
    Restore,
}

impl Cli {
    pub fn into_request(self) -> protocol::Request {
        match self.command {
            Commands::Sticky { action } => match action {
                StickyAction::Add { window_id } => protocol::Request::Add { window_id },
                StickyAction::Remove { window_id } => protocol::Request::Remove { window_id },
                StickyAction::List => protocol::Request::List,
                StickyAction::ToggleActive => protocol::Request::ToggleActive,
                StickyAction::ToggleAppid { appid } => protocol::Request::ToggleAppid { appid },
                StickyAction::ToggleTitle { title } => protocol::Request::ToggleTitle { title },
            },
            Commands::Stage { action } => match action {
                StageAction::List => protocol::Request::StageList,
                StageAction::Add { window_id } => protocol::Request::Stage { window_id },
                StageAction::Remove { window_id } => protocol::Request::Unstage { window_id },
                StageAction::ToggleActive => protocol::Request::StageToggleActive,
                StageAction::ToggleAppid { appid } => protocol::Request::StageToggleAppid { appid },
                StageAction::ToggleTitle { title } => protocol::Request::StageToggleTitle { title },
                StageAction::AddAll => protocol::Request::StageAll,
                StageAction::RemoveAll => protocol::Request::UnstageAll,
                StageAction::Restore => unreachable!("Restore is handled separately"),
            },
            Commands::Windows { .. } => protocol::Request::Windows,
        }
    }
}

async fn daemon_exchange(req: &protocol::Request) -> Result<protocol::Response> {
    let stream = UnixStream::connect(crate::protocol::CLI_SOCKET_PATH).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let json = serde_json::to_string(req)?;
    writer.write_all(json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;
    let mut line = String::new();
    reader.read_line(&mut line).await?;
    Ok(serde_json::from_str(line.trim())?)
}

// 无 NSTICKY_MENU 时的纯终端交互：输入编号（空格分隔可多选），q 取消。
async fn run_terminal_restore(staged: Vec<WindowInfo>) -> Result<()> {
    println!();
    for (i, w) in staged.iter().enumerate() {
        let app_id = w.app_id.as_deref().unwrap_or("?");
        let title = w.title.as_deref().unwrap_or("?");
        println!(" {}. {} — {}  [ID: {}]", i + 1, app_id, title, w.id);
    }
    println!();
    print!(
        "Restore (1-{}, space-separated for multiple, q): ",
        staged.len()
    );
    std::io::stdout().flush()?;
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim();
    if input.eq_ignore_ascii_case("q") || input.is_empty() {
        return Ok(());
    }
    for tok in input.split_whitespace() {
        if let Ok(n) = tok.parse::<usize>()
            && n >= 1
            && n <= staged.len()
        {
            let w = &staged[n - 1];
            match daemon_exchange(&protocol::Request::Unstage { window_id: w.id }).await? {
                protocol::Response::Success { message } => println!("{message}"),
                protocol::Response::Error { message } => eprintln!("Error: {message}"),
                _ => {}
            }
        }
    }
    Ok(())
}

async fn run_stage_restore(menu: Option<String>) -> Result<()> {
    let ids: Vec<u64> = match daemon_exchange(&protocol::Request::StageList).await? {
        protocol::Response::Data { data } => serde_json::from_str(&data)?,
        protocol::Response::Error { message } => {
            eprintln!("Error: {message}");
            return Ok(());
        }
        _ => return Ok(()),
    };

    // 选择器来源：环境变量 NSTICKY_MENU 优先，其次 config 的 menu 字段；
    // 都没有则退回纯终端交互。
    let selector = std::env::var("NSTICKY_MENU")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .or(menu);

    match selector {
        // 有外部选择器时，即使没有 staged 窗口也照常打开，让它显示空态，
        // 否则绑定快捷键后会「没反应」，用户无从感知。
        Some(menu) => {
            let all: Vec<WindowInfo> = match daemon_exchange(&protocol::Request::Windows).await? {
                protocol::Response::Data { data } => serde_json::from_str(&data)?,
                protocol::Response::Error { message } => {
                    eprintln!("Error: {message}");
                    return Ok(());
                }
                _ => return Ok(()),
            };
            let staged: Vec<WindowInfo> = all.into_iter().filter(|w| ids.contains(&w.id)).collect();
            run_menu_restore(&menu, &staged).await
        }
        // 无选择器（纯终端）才在此短路，避免空列表刷屏。
        None => {
            if ids.is_empty() {
                println!("No staged windows.");
                return Ok(());
            }
            let all: Vec<WindowInfo> = match daemon_exchange(&protocol::Request::Windows).await? {
                protocol::Response::Data { data } => serde_json::from_str(&data)?,
                protocol::Response::Error { message } => {
                    eprintln!("Error: {message}");
                    return Ok(());
                }
                _ => return Ok(()),
            };
            let staged: Vec<WindowInfo> = all.into_iter().filter(|w| ids.contains(&w.id)).collect();
            if staged.is_empty() {
                println!("No active staged windows found.");
                return Ok(());
            }
            run_terminal_restore(staged).await
        }
    }
}

// 把 staged 列表（每行 "<id>\t<app_id> — <title>"）喂给外部选择器，
// 读取其 stdout 逐行提取 window ID 并 restore（支持选择器多选输出多行）。
async fn run_menu_restore(menu: &str, staged: &[WindowInfo]) -> Result<()> {
    use std::io::Write;
    use std::process::{Command, Stdio};

    // 菜单程序名可能带参数（如 "fuzzel --dmenu"、"rofi -dmenu"），
    // 按空白分词成 program + args，不能整体当一个可执行文件名。
    let mut parts = menu.split_whitespace();
    let Some(program) = parts.next() else {
        return Ok(());
    };
    let args: Vec<&str> = parts.collect();

    let mut child = Command::new(program)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to spawn selector '{menu}'"))?;

    let input = staged
        .iter()
        .map(|w| {
            format!(
                "{}\t{} — {}",
                w.id,
                w.app_id.as_deref().unwrap_or("?"),
                w.title.as_deref().unwrap_or("?")
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(input.as_bytes())?;
    }

    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Ok(());
    }

    let re = regex::Regex::new(r"\d+").ok();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some(re) = &re
            && let Some(cap) = re.find(line.trim())
            && let Ok(id) = cap.as_str().parse::<u64>()
        {
            match daemon_exchange(&protocol::Request::Unstage { window_id: id }).await? {
                protocol::Response::Success { message } => println!("{message}"),
                protocol::Response::Error { message } => eprintln!("Error: {message}"),
                _ => {}
            }
        }
    }
    Ok(())
}

pub async fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    if let Commands::Stage {
        action: StageAction::Restore,
    } = &cli.command
    {
        let menu = crate::config::Config::load_or_default()
            .menu()
            .map(String::from);
        return run_stage_restore(menu).await;
    }

    let filter_args = match &cli.command {
        Commands::Windows { app_id, title } => (app_id.clone(), title.clone()),
        _ => (None, None),
    };

    let socket_path = crate::protocol::CLI_SOCKET_PATH;
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    let request = cli.into_request();

    let is_windows = matches!(request, protocol::Request::Windows);

    let request_json = serde_json::to_string(&request)?;
    writer.write_all(request_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    let mut response = String::new();
    reader.read_line(&mut response).await?;
    let response = response.trim();

    let parsed: protocol::Response =
        serde_json::from_str(response).context("Invalid JSON response from daemon")?;
    match parsed {
        protocol::Response::Success { message } => println!("{message}"),
        protocol::Response::Error { message } => {
            eprintln!("Error: {message}");
        }
        protocol::Response::Data { data } => {
            if is_windows {
                let windows: Vec<WindowInfo> =
                    serde_json::from_str(&data).context("Failed to parse window list")?;
                let (ref filter_app_id, ref filter_title) = filter_args;
                let filtered: Vec<_> = windows
                    .into_iter()
                    .filter(|w| {
                        let app_match = match (filter_app_id, &w.app_id) {
                            (Some(f), Some(id)) => id.to_lowercase().contains(&f.to_lowercase()),
                            (Some(_), None) => false,
                            (None, _) => true,
                        };
                        let title_match = match (filter_title, &w.title) {
                            (Some(f), Some(t)) => t.to_lowercase().contains(&f.to_lowercase()),
                            (Some(_), None) => false,
                            (None, _) => true,
                        };
                        app_match && title_match
                    })
                    .collect();
                println!("{:<10} {:<25} TITLE", "ID", "APP_ID");
                for w in filtered {
                    let app_id = w.app_id.as_deref().unwrap_or("<unknown>");
                    let title = w.title.as_deref().unwrap_or("<unknown>");
                    println!("{:<10} {:<25} {}", w.id, app_id, title);
                }
            } else {
                println!("{data}");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_sticky_add() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "add", "42"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::Add { window_id: 42 });
    }

    #[test]
    fn test_cli_sticky_remove() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "remove", "7"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::Remove { window_id: 7 }
        );
    }

    #[test]
    fn test_cli_sticky_list() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "list"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::List);
    }

    #[test]
    fn test_cli_sticky_toggle_active() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "toggle-active"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::ToggleActive);
    }

    #[test]
    fn test_cli_sticky_toggle_appid() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "toggle-appid", "firefox"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::ToggleAppid {
                appid: "firefox".to_string()
            }
        );
    }

    #[test]
    fn test_cli_sticky_toggle_title() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "toggle-title", "Gmail"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::ToggleTitle {
                title: "Gmail".to_string()
            }
        );
    }

    #[test]
    fn test_cli_stage_list() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "list"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::StageList);
    }

    #[test]
    fn test_cli_stage_add() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "add", "99"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::Stage { window_id: 99 }
        );
    }

    #[test]
    fn test_cli_stage_remove() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "remove", "10"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::Unstage { window_id: 10 }
        );
    }

    #[test]
    fn test_cli_stage_toggle_active() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "toggle-active"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::StageToggleActive);
    }

    #[test]
    fn test_cli_stage_toggle_appid() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "toggle-appid", "chromium"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::StageToggleAppid {
                appid: "chromium".to_string()
            }
        );
    }

    #[test]
    fn test_cli_stage_toggle_title() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "toggle-title", "Terminal"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::StageToggleTitle {
                title: "Terminal".to_string()
            }
        );
    }

    #[test]
    fn test_cli_stage_add_all() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "add-all"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::StageAll);
    }

    #[test]
    fn test_cli_stage_remove_all() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "remove-all"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::UnstageAll);
    }

    #[test]
    fn test_cli_windows() {
        let cli = Cli::try_parse_from(["nsticky", "windows"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::Windows);
        let cli = Cli::try_parse_from(["nsticky", "windows", "--app-id", "firefox"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::Windows);
        let cli = Cli::try_parse_from(["nsticky", "windows", "--title", "gmail"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::Windows);
    }

    #[test]
    fn test_cli_stage_restore() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "restore"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Stage {
                action: StageAction::Restore
            }
        ));
    }

    #[test]
    fn test_cli_stage_restore_alias() {
        let cli = Cli::try_parse_from(["nsticky", "stage", "rs"]).unwrap();
        assert!(matches!(
            cli.command,
            Commands::Stage {
                action: StageAction::Restore
            }
        ));
    }

    #[test]
    fn test_cli_aliases() {
        let cli = Cli::try_parse_from(["nsticky", "sticky", "a", "5"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::Add { window_id: 5 });
        let cli = Cli::try_parse_from(["nsticky", "sticky", "r", "3"]).unwrap();
        assert_eq!(
            cli.into_request(),
            protocol::Request::Remove { window_id: 3 }
        );
        let cli = Cli::try_parse_from(["nsticky", "sticky", "l"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::List);
        let cli = Cli::try_parse_from(["nsticky", "sticky", "t"]).unwrap();
        assert_eq!(cli.into_request(), protocol::Request::ToggleActive);
    }
}
