use std::{fmt, io::Write};

mod commands;
use commands::check_all_commands;

enum DoctorError {
    BinaryNotFound(FailedCheck),
}

impl DoctorError {
    pub fn is_fatal(&self) -> bool {
        match self {
            DoctorError::BinaryNotFound(failed_check) => match failed_check.required_bin.gravity {
                Gravity::Needed => true,
                Gravity::Recommended => false,
            },
        }
    }
}

#[derive(Clone)]
struct FailedCheck {
    required_bin: RequiredBin,
    check: Check,
    status_code: i32,
    stderr: String,
}

#[derive(Clone, Copy)]
struct Check {
    command: &'static str,
    args: &'static [&'static str],
}

#[derive(Clone, Copy)]
struct RequiredBin {
    checks: &'static [Check],
    purpose: &'static str,
    notes: &'static str,
    gravity: Gravity,
}

#[derive(Clone, Copy)]
enum Gravity {
    Needed,
    Recommended,
}

impl fmt::Display for DoctorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DoctorError::BinaryNotFound(failed_check) => write!(
                f,
                "\x1b[31mRequired binary '\x1b[1m{}\x1b[0m\x1b[31m' not found (status code: {}): {}\x1b[0m\n\x1b[33mPurpose: {}\x1b[0m\n\x1b[36mInstallation: {}\x1b[0m",
                failed_check.check.command,
                failed_check.status_code,
                failed_check.stderr,
                failed_check.required_bin.purpose,
                failed_check.required_bin.notes
            ),
        }
    }
}

pub(crate) async fn doctor() -> Result<(), i32> {
    // rule: we only output to stderr, never stdout

    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    eprint!("  Verifying your installation... ");
    let done = Arc::new(AtomicBool::new(false));
    let spinner_done = done.clone();

    let spinner = std::thread::spawn(move || {
        let spinner = ['|', '/', '-', '\\'];
        let mut i = 0;
        while !spinner_done.load(Ordering::Relaxed) {
            eprint!("\r{}", spinner[i]);
            i = (i + 1) % spinner.len();
            std::io::stderr().flush().unwrap();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    let mut errors = Vec::new();
    errors.extend(check_all_commands().await);

    done.store(true, Ordering::Relaxed);
    spinner.join().unwrap();
    eprint!("\r                                               \r");
    std::io::stderr().flush().unwrap();

    if !errors.is_empty() {
        eprintln!("\nDoctor encountered {} errors", errors.len());

        let mut fatal_errors = 0;
        for (index, error) in errors.iter().enumerate() {
            eprintln!("⭕ Error {} of {}", index + 1, errors.len());
            eprintln!("\x1b[31m{error}\x1b[0m");
            if error.is_fatal() {
                fatal_errors += 1;
            }
        }

        if fatal_errors > 0 {
            eprintln!(
                "\n{} of {} errors were fatal, so we're going to quit now.",
                fatal_errors,
                errors.len()
            );
            Err(1)
        } else {
            eprintln!("\nNone of those errors are fatal, so we're good to go. Welcome home!");
            Ok(())
        }
    } else {
        eprintln!("\x1b[32mAll checks passed successfully\x1b[0m");
        Ok(())
    }
}
