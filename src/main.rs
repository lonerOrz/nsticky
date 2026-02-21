mod business;
mod cli;
mod config;
mod daemon;
mod protocol;
mod system_integration;

use anyhow::Result;
use std::{collections::HashSet, env, sync::Arc};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<()> {
    if env::args().nth(1).is_some() {
        // Run in CLI mode
        return cli::run_cli().await;
    }

    // Run in daemon mode
    let sticky_windows = Arc::new(Mutex::new(HashSet::<u64>::new()));

    daemon::start(sticky_windows).await
}
