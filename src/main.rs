//! Audiotester - Windows ASIO audio testing application
//!
//! Entry point for the system tray application.

use anyhow::Result;
use audiotester::audio::engine::AudioEngine;
use audiotester::ui::tray::{TrayAction, TrayManager, TrayStatus};
use std::io::{self, Write};
use std::time::Duration;
use tracing::{error, info};

fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("audiotester=info".parse().unwrap()),
        )
        .init();

    println!("╔════════════════════════════════════════════════════════════╗");
    println!(
        "║           Audiotester v{} - ASIO Audio Monitor          ║",
        audiotester::VERSION
    );
    println!("╚════════════════════════════════════════════════════════════╝");
    println!();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();

    // Parse options
    let mut device_name: Option<String> = None;
    let mut sample_rate: Option<u32> = None;
    let mut no_tray = false;
    let mut i = 1;

    while i < args.len() {
        match args[i].as_str() {
            "--list" | "-l" => {
                list_devices()?;
                return Ok(());
            }
            "--version" | "-v" => {
                println!("audiotester {}", audiotester::VERSION);
                return Ok(());
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            "--device" | "-d" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --device requires a device name");
                    return Ok(());
                }
                device_name = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
            "--sample-rate" | "-r" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --sample-rate requires a value");
                    return Ok(());
                }
                sample_rate = args[i + 1].parse().ok();
                if sample_rate.is_none() {
                    eprintln!("Error: Invalid sample rate: {}", args[i + 1]);
                    return Ok(());
                }
                i += 2;
                continue;
            }
            "--no-tray" => {
                no_tray = true;
                i += 1;
                continue;
            }
            arg if arg.starts_with('-') => {
                eprintln!("Unknown argument: {}", arg);
                print_help();
                return Ok(());
            }
            _ => {
                // Positional argument - treat as device name if not set
                if device_name.is_none() {
                    device_name = Some(args[i].clone());
                }
            }
        }
        i += 1;
    }

    // If device specified via command line, run with it
    if let Some(dev) = device_name {
        run_with_device(&dev, sample_rate, no_tray)?;
        return Ok(());
    }

    // Interactive mode
    interactive_mode()
}

fn print_help() {
    println!("Usage: audiotester [OPTIONS]");
    println!();
    println!("Options:");
    println!("  -l, --list              List available ASIO devices");
    println!("  -d, --device NAME       Start monitoring with specified device");
    println!("  -r, --sample-rate RATE  Set sample rate (default: 96000)");
    println!("      --no-tray           Disable system tray icon (console only)");
    println!("  -v, --version           Show version");
    println!("  -h, --help              Show this help");
    println!();
    println!("Examples:");
    println!("  audiotester -d \"VB-Matrix VASIO-8\" -r 96000");
    println!("  audiotester --list");
    println!();
    println!("Without arguments, starts in interactive mode.");
}

fn list_devices() -> Result<()> {
    println!("Scanning for ASIO devices...");
    println!();

    match AudioEngine::list_devices() {
        Ok(devices) => {
            if devices.is_empty() {
                println!("No ASIO devices found.");
                println!();
                println!("Make sure:");
                println!("  1. ASIO drivers are installed");
                println!("  2. Audio interface is connected");
                println!("  3. ASIO4ALL is installed (for non-ASIO hardware)");
            } else {
                println!("Found {} device(s):", devices.len());
                println!();
                for (i, device) in devices.iter().enumerate() {
                    let default_marker = if device.is_default { " [DEFAULT]" } else { "" };
                    println!("  {}. {}{}", i + 1, device.name, default_marker);
                    println!(
                        "     Channels: {} in, {} out",
                        device.input_channels, device.output_channels
                    );
                    if !device.sample_rates.is_empty() {
                        println!("     Sample rates: {:?}", device.sample_rates);
                    }
                    println!();
                }
            }
        }
        Err(e) => {
            error!("Failed to list devices: {}", e);
            println!("Error: {}", e);
            println!();
            println!("ASIO may not be available on this system.");
        }
    }

    Ok(())
}

fn run_with_device(device_name: &str, sample_rate: Option<u32>, no_tray: bool) -> Result<()> {
    println!("Starting with device: {}", device_name);
    if let Some(rate) = sample_rate {
        println!("Sample rate: {} Hz", rate);
    } else {
        println!(
            "Sample rate: {} Hz (default)",
            audiotester::DEFAULT_SAMPLE_RATE
        );
    }
    println!();

    let mut engine = AudioEngine::new();

    // Set sample rate if specified
    if let Some(rate) = sample_rate {
        engine.set_sample_rate(rate);
    }

    // Select device
    if let Err(e) = engine.select_device(device_name) {
        error!("Failed to select device: {}", e);
        println!("Error: Could not find device '{}'", device_name);
        println!();
        println!("Use --list to see available devices.");
        return Ok(());
    }

    // Start engine
    if let Err(e) = engine.start() {
        error!("Failed to start engine: {}", e);
        println!("Error: {}", e);
        return Ok(());
    }

    info!("Output callback active - sending MLS signal on channel 1");

    // Initialize tray manager
    let mut tray_manager = TrayManager::new()?;
    let use_tray = !no_tray && cfg!(windows);

    if use_tray {
        match tray_manager.init(Some(device_name)) {
            Ok(()) => {
                tray_manager.set_monitoring(true);
                tray_manager.set_status(TrayStatus::Ok)?;
                info!("System tray icon initialized");
            }
            Err(e) => {
                error!("Failed to initialize tray icon: {}", e);
                println!("Warning: Could not create system tray icon: {}", e);
            }
        }
    }

    println!("Monitoring started. Press Ctrl+C to stop.");
    if use_tray {
        println!("System tray icon active - right-click for menu.");
    }
    println!();
    println!("Status:");
    println!("────────────────────────────────────────");

    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })
    .ok();

    // Main monitoring loop
    let mut last_status = String::new();
    let mut iteration = 0u32;
    while running.load(std::sync::atomic::Ordering::SeqCst) {
        // Check for tray menu events
        if use_tray {
            if let Some(action) = tray_manager.poll_event() {
                match action {
                    TrayAction::Exit => {
                        info!("Exit requested from tray menu");
                        break;
                    }
                    TrayAction::ToggleMonitoring => {
                        if tray_manager.is_monitoring() {
                            info!("Stopping monitoring from tray menu");
                            engine.stop()?;
                            tray_manager.set_monitoring(false);
                            tray_manager.set_status(TrayStatus::Disconnected)?;
                        } else {
                            info!("Starting monitoring from tray menu");
                            engine.start()?;
                            tray_manager.set_monitoring(true);
                            tray_manager.set_status(TrayStatus::Ok)?;
                        }
                    }
                    TrayAction::ShowStats => {
                        info!("Show stats requested (not yet implemented)");
                        // TODO: Phase 5 - Open stats window
                    }
                    TrayAction::SelectDevice => {
                        info!("Select device requested (not yet implemented)");
                        // TODO: Open device selection dialog
                    }
                }
            }
        }

        // Skip analysis if not monitoring
        if use_tray && !tray_manager.is_monitoring() {
            #[cfg(windows)]
            {
                use windows::Win32::UI::WindowsAndMessaging::{
                    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
                };
                unsafe {
                    let mut msg = MSG::default();
                    while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                        let _ = TranslateMessage(&msg);
                        let _ = DispatchMessageW(&msg);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }

        let (out_samples, in_samples) = engine.sample_counts();

        // Every 10 iterations, show sample counts for debugging
        if iteration > 0 && iteration.is_multiple_of(10) {
            info!(
                "Audio I/O: {} samples out, {} samples in",
                out_samples, in_samples
            );
        }
        iteration += 1;

        if let Some(result) = engine.analyze() {
            let (status, tray_status) = if result.is_healthy {
                ("OK", TrayStatus::Ok)
            } else if result.lost_samples > 0 {
                ("LOSS", TrayStatus::Error)
            } else if result.confidence < 0.5 {
                ("LOW", TrayStatus::Warning)
            } else {
                ("WARN", TrayStatus::Warning)
            };

            // Update tray icon based on status
            if use_tray {
                let _ = tray_manager.set_status(tray_status);
                let tooltip = format!(
                    "Audiotester - {}ms latency, {:.0}% confidence",
                    result.latency_ms,
                    result.confidence * 100.0
                );
                let _ = tray_manager.set_tooltip(&tooltip);
                let status_text = format!(
                    "Latency: {:.2}ms | Confidence: {:.0}%",
                    result.latency_ms,
                    result.confidence * 100.0
                );
                let _ = tray_manager.set_status_text(&status_text);
            }

            let status_line = format!(
                "Latency: {:>6.2}ms | Lost: {:>4} | Confidence: {:>5.1}% | Out: {:>8} | In: {:>8} | Status: {}",
                result.latency_ms,
                result.lost_samples,
                result.confidence * 100.0,
                out_samples,
                in_samples,
                status
            );

            // Only print if changed (reduce spam)
            if status_line != last_status {
                println!("{}", status_line);
                last_status = status_line;
            }
        }

        // Process Windows messages (required for tray icon menu to work)
        #[cfg(windows)]
        {
            use windows::Win32::UI::WindowsAndMessaging::{
                DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE,
            };
            unsafe {
                let mut msg = MSG::default();
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).into() {
                    let _ = TranslateMessage(&msg);
                    let _ = DispatchMessageW(&msg);
                }
            }
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    println!();
    println!("Stopping...");
    engine.stop()?;
    println!("Done.");

    Ok(())
}

fn interactive_mode() -> Result<()> {
    println!("Interactive Mode");
    println!("────────────────────────────────────────");
    println!();

    // List devices first
    list_devices()?;

    let devices = AudioEngine::list_devices().unwrap_or_default();
    if devices.is_empty() {
        println!("No devices available. Exiting.");
        return Ok(());
    }

    // Prompt for device selection
    println!();
    print!("Enter device number (1-{}): ", devices.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    let device_num: usize = match input.trim().parse() {
        Ok(n) if n >= 1 && n <= devices.len() => n,
        _ => {
            println!("Invalid selection. Exiting.");
            return Ok(());
        }
    };

    let device_name = &devices[device_num - 1].name;
    run_with_device(device_name, None, false)
}
