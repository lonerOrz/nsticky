mod business;
mod cli;
mod config;
mod daemon;
mod protocol;
mod system_integration;

use anyhow::Result;
use std::env;

fn main() -> Result<()> {
    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_target(false)
        .with_line_number(false)
        .without_time()
        .init();

    if env::args().nth(1).is_some() {
        // Run in CLI mode
        return tokio::runtime::Runtime::new()?.block_on(cli::run_cli());
    }

    // Run in daemon mode
    tokio::runtime::Runtime::new()?.block_on(daemon::start())
}
