mod cli;
mod daemon;

use anyhow::Result;
use std::{collections::HashSet, env, sync::Arc};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    if env::args().nth(1).is_some() {
        // 有参数，运行cli
        return cli::run_cli().await;
    }

    // 守护进程模式
    let sticky_windows = Arc::new(Mutex::new(HashSet::<u64>::new()));

    daemon::start(sticky_windows).await
}
