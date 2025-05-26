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

    let status = match std::process::Command::new(format!("home-{subcommand}"))
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
