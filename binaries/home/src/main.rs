fn main() {
    let mut args = std::env::args().skip(1);
    let subcommand = match args.next() {
        Some(s) => s,
        None => {
            eprintln!("Missing subcommand");
            std::process::exit(1);
        }
    };
    let args: Vec<String> = args.collect();

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to get current executable path: {e}");
            std::process::exit(1);
        }
    };

    let exe_dir = match exe_path.parent() {
        Some(dir) => dir,
        None => {
            eprintln!("Failed to get parent directory of current executable");
            std::process::exit(1);
        }
    };

    let sub_exe_path = exe_dir.join(format!("home-{subcommand}"));

    let status = match std::process::Command::new(sub_exe_path)
        .args(&args)
        .status()
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Failed to execute command: {e}");
            std::process::exit(1);
        }
    };
    if !status.success() {
        eprintln!("Command exited with a non-zero status code");
        std::process::exit(1);
    }
}
