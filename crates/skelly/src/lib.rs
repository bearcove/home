pub use color_eyre;
pub use color_eyre::eyre;
use liberrhandling::should_include_frame_name;
pub use log;
pub use owo_colors;

use log::{Level, LevelFilter, Log, Metadata, Record};
use owo_colors::{OwoColorize, Style};
use std::{io::Write, time::Duration};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

struct SimpleLogger;

impl Log for SimpleLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        // Only log entries at or above the max level filter from log.
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        // Create style based on log level
        let level_style = match record.level() {
            Level::Error => Style::new().fg_rgb::<243, 139, 168>(), // Catppuccin red (Maroon)
            Level::Warn => Style::new().fg_rgb::<249, 226, 175>(),  // Catppuccin yellow (Peach)
            Level::Info => Style::new().fg_rgb::<166, 227, 161>(),  // Catppuccin green (Green)
            Level::Debug => Style::new().fg_rgb::<137, 180, 250>(), // Catppuccin blue (Blue)
            Level::Trace => Style::new().fg_rgb::<148, 226, 213>(), // Catppuccin teal (Teal)
        };

        // Convert level to styled display
        eprintln!(
            "{} - {}: {}",
            record.level().style(level_style),
            record
                .target()
                .style(Style::new().fg_rgb::<137, 180, 250>()), // Blue for the target
            record.args()
        );
    }

    fn flush(&self) {
        let _ = std::io::stderr().flush();
    }
}

/// Installs color-backtrace (except on miri), and sets up a simple logger.
pub fn setup() {
    use color_eyre::config::HookBuilder;

    // color-eyre filter
    let eyre_filter = {
        move |frames: &mut Vec<&color_eyre::config::Frame>| {
            // eprintln!("[skelly] color-eyre filter called!");
            frames.retain(|frame| {
                frame
                    .name
                    .as_ref()
                    .map(should_include_frame_name)
                    .unwrap_or(true)
            });
        }
    };

    HookBuilder::default()
        .add_frame_filter(Box::new(eyre_filter))
        .install()
        .expect("Failed to set up color-eyre");

    // color-backtrace filter
    {
        use color_backtrace::{BacktracePrinter, Frame};

        // The frame filter must be Fn(&mut Vec<&Frame>)
        let filter = move |frames: &mut Vec<&Frame>| {
            // eprintln!("[skelly] color-backtrace filter called");
            frames.retain(|frame| {
                frame
                    .name
                    .as_ref()
                    .map(should_include_frame_name)
                    .unwrap_or(true)
            });
        };

        // Build and install custom BacktracePrinter with our filter.
        // Use StandardStream to provide a WriteColor.
        let stderr = color_backtrace::termcolor::StandardStream::stderr(
            color_backtrace::termcolor::ColorChoice::Auto,
        );
        let printer = BacktracePrinter::new().add_frame_filter(Box::new(filter));
        printer.install(Box::new(stderr));
    }

    let logger = sentry::integrations::log::SentryLogger::with_dest(SimpleLogger);
    log::set_boxed_logger(Box::new(logger)).unwrap();

    // Respect RUST_LOG, fallback to Trace if not set or invalid
    let level = std::env::var("RUST_LOG")
        .ok()
        .and_then(|val| val.parse::<LevelFilter>().ok())
        .unwrap_or(LevelFilter::Info);

    log::set_max_level(level);

    // Watch parent process if SKELLY_PARENT_PID is set
    if let Ok(parent_pid_str) = std::env::var("SKELLY_PARENT_PID") {
        if let Ok(parent_pid) = parent_pid_str.parse::<usize>() {
            log::debug!("Watching parent process with PID {parent_pid}");

            std::thread::spawn(move || {
                watch_parent_process(parent_pid);
            });
        } else {
            log::warn!("Invalid SKELLY_PARENT_PID value: {parent_pid_str}");
        }
    }
}

fn watch_parent_process(parent_pid: usize) {
    let pid = Pid::from(parent_pid);
    let mut system = System::new();

    loop {
        system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            true,
            ProcessRefreshKind::nothing(),
        );

        if system.process(pid).is_none() {
            log::warn!("Parent process (PID {parent_pid}) has exited, terminating");
            std::process::exit(1);
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}

/// Spawn a child process with SKELLY_PARENT_PID set to the current process ID
pub fn spawn(mut cmd: tokio::process::Command) -> tokio::process::Command {
    let current_pid = std::process::id();
    cmd.env("SKELLY_PARENT_PID", current_pid.to_string());
    cmd
}
