//! Audiotester - Windows ASIO audio testing application
//!
//! Entry point for the system tray application.

use anyhow::Result;
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

fn main() -> Result<()> {
    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("audiotester=debug".parse().unwrap()),
        )
        .init();

    info!("Audiotester v{} starting...", audiotester::VERSION);

    // TODO: Phase 4 - Initialize system tray
    // TODO: Phase 2 - Initialize audio engine
    // TODO: Phase 5 - Initialize statistics window

    info!("Audiotester initialized successfully");

    // Placeholder: Run event loop
    // The actual event loop will be implemented in Phase 4 with tray-icon
    println!(
        "Audiotester v{} - Press Ctrl+C to exit",
        audiotester::VERSION
    );

    // Keep running until interrupted
    std::thread::park();

    Ok(())
}
