include!(".dylo/spec.rs");
include!(".dylo/support.rs");

use clap::TermArgs;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum FormatAnsiStyle {
    Markdown,
    Html,
}

#[dylo::export]
impl Mod for ModImpl {
    fn css(&self) -> String {
        use std::fmt::Write;

        let mut output = String::new();

        writeln!(output, "/* ANSI colors start */").unwrap();
        writeln!(output, ".home-ansi i {{ font-style: normal !important; }}").unwrap();
        writeln!(output).unwrap();
        for code in 0..=255 {
            let mut c_light = coolor::AnsiColor::new(code).to_hsl();
            c_light.l *= 0.8;
            c_light.s *= 0.6;
            let rgb_light = c_light.to_rgb();
            let hex_light = format!("#{:02x}{:02x}{:02x}", rgb_light.r, rgb_light.g, rgb_light.b);

            let mut c_dark = coolor::AnsiColor::new(code).to_hsl();
            c_dark.l *= 1.5;
            c_dark.l = c_dark.l.clamp(0.0, 1.0);
            c_dark.s *= 0.65;
            let rgb_dark = c_dark.to_rgb();
            let hex_dark = format!("#{:02x}{:02x}{:02x}", rgb_dark.r, rgb_dark.g, rgb_dark.b);

            writeln!(
                output,
                ".home-ansi i.fg-ansi{code} {{ color: light-dark({hex_light}, {hex_dark}); }}"
            )
            .unwrap();
            writeln!(
                    output,
                    ".home-ansi i.bg-ansi{code} {{ background-color: light-dark({hex_light}, {hex_dark}); }}"
                ).unwrap();
        }
        writeln!(output).unwrap();

        writeln!(
            output,
            "//------------------------------------------------------------------------------"
        )
        .unwrap();
        writeln!(output, "/* ANSI colors end */").unwrap();
        writeln!(
            output,
            "//------------------------------------------------------------------------------"
        )
        .unwrap();

        // Classic colors
        writeln!(
            output,
            ".home-ansi i.fg-blk {{ color: light-dark(oklch(0.2 0 0), oklch(0.8 0 0)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-red {{ color: light-dark(oklch(0.6 0.2 25), oklch(0.7 0.2 25)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-grn {{ color: light-dark(oklch(0.7 0.2 140), oklch(0.8 0.2 140)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-ylw {{ color: light-dark(oklch(0.8 0.2 90), oklch(0.9 0.2 90)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-blu {{ color: light-dark(oklch(0.6 0.2 250), oklch(0.7 0.2 250)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-mag {{ color: light-dark(oklch(0.7 0.2 320), oklch(0.8 0.2 320)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-cyn {{ color: light-dark(oklch(0.8 0.2 200), oklch(0.9 0.2 200)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-wht {{ color: light-dark(oklch(0.9 0 0), oklch(0.1 0 0)); }}"
        )
        .unwrap();

        writeln!(output).unwrap();

        // Light versions
        writeln!(
            output,
            ".home-ansi i.fg-lblk {{ color: light-dark(oklch(0.4 0 0), oklch(0.6 0 0)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lred {{ color: light-dark(oklch(0.7 0.2 25), oklch(0.8 0.2 25)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lgrn {{ color: light-dark(oklch(0.8 0.2 140), oklch(0.9 0.2 140)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lyel {{ color: light-dark(oklch(0.9 0.2 90), oklch(1.0 0.2 90)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lblu {{ color: light-dark(oklch(0.7 0.2 250), oklch(0.8 0.2 250)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lmag {{ color: light-dark(oklch(0.8 0.2 320), oklch(0.9 0.2 320)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lcyn {{ color: light-dark(oklch(0.9 0.2 200), oklch(1.0 0.2 200)); }}"
        )
        .unwrap();
        writeln!(
            output,
            ".home-ansi i.fg-lwht {{ color: light-dark(oklch(1.0 0 0), oklch(0.0 0 0)); }}"
        )
        .unwrap();

        output
    }

    fn format_ansi(&self, ansi: &str, style: FormatAnsiStyle) -> String {
        let mut performer = impls::Performer {
            strict: false,
            ..Default::default()
        };

        let mut parser = anstyle_parse::Parser::<anstyle_parse::Utf8Parser>::new();
        for byte in ansi.bytes() {
            if byte == b'\n' {
                parser.advance(&mut performer, b'\r');
                parser.advance(&mut performer, b'\n');
            } else {
                parser.advance(&mut performer, byte);
            }
        }

        performer.finish(style)
    }

    fn run(&self, args: TermArgs) {
        use owo_colors::OwoColorize;
        use std::io::{Read, Write};
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        if args.css {
            println!("{}", self.css());
            std::process::exit(0);
        }

        eprintln!("✂️ Welcome to terminus, enjoy your favorite shell (set `$SHELL` to override)");

        let child_args = args.args;
        let pty_system = portable_pty::native_pty_system();

        let (w, h) = term_size::dimensions().unwrap_or((80, 24));

        let pair = pty_system
            .openpty(portable_pty::PtySize {
                rows: h as _,
                cols: w as _,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let mut cmd = portable_pty::CommandBuilder::new(shell);
        // pagers make terminus hang
        cmd.env("PAGER", "/bin/cat");
        cmd.env("GIT_PAGER", "/bin/cat");
        cmd.env("DELTA_PAGER", "/bin/cat");
        // force dark mode and colorblindness-friendly color scheme, cf.
        // https://github.com/fasterthanlime/dotfiles/commit/72448e9fd4c920d7fff49b1edc2b6c71e1b4133c
        cmd.env("DELTA_FEATURES", "snow-panther");
        cmd.cwd(std::env::current_dir().unwrap());

        let mut child = pair.slave.spawn_command(cmd).unwrap();
        let shell_pid = child.process_id().unwrap();

        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();

        let mut stdin = std::io::stdin();
        use termion::raw::IntoRawMode;
        let mut stdout = std::io::stdout().into_raw_mode().unwrap();

        let (output_tx, output_rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();

        // read from the pty and write to output_tx
        {
            let output_tx = output_tx.clone();
            std::thread::spawn(move || {
                let mut buf = vec![0; 1024];
                loop {
                    let n = reader.read(&mut buf).unwrap();
                    if n == 0 {
                        _ = output_tx.send(None);
                        break;
                    }
                    output_tx.send(Some(buf[..n].to_vec())).unwrap();
                }
            });
        }

        let last_stdout_activity = Arc::new(Mutex::new(Instant::now()));

        // read from output_rx and echo to stdout + collect into out
        let collect_output_jh = {
            let mut out = vec![];
            let last_stdout_activity = last_stdout_activity.clone();

            std::thread::spawn(move || {
                for buf in output_rx {
                    if let Some(buf) = buf {
                        *last_stdout_activity.lock().unwrap() = Instant::now();
                        stdout.write_all(&buf).unwrap();
                        stdout.flush().unwrap();
                        out.extend_from_slice(&buf);
                    } else {
                        break;
                    }
                }

                (out, stdout)
            })
        };

        let (stdin_tx, stdin_rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();

        // read from stdin and send to channel
        {
            let stdin_tx = stdin_tx.clone();
            std::thread::spawn(move || {
                if !child_args.is_empty() {
                    // wait for stdout to be inactive
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(25));

                        let last_activity = *last_stdout_activity.lock().unwrap();
                        if last_activity.elapsed().as_millis() > 350 {
                            break;
                        }
                    }

                    // quote
                    let mut script: String = "".to_owned();
                    for (i, arg) in child_args.iter().enumerate() {
                        if i > 0 {
                            script.push(' ');
                        }
                        use shell_quote::{Bash, QuoteExt};
                        script.push_quoted(Bash, arg);
                    }
                    script.push('\n');
                    stdin_tx.send(Some(script.into_bytes())).unwrap();

                    // After script sent, wait for stdout activity to stop and for all children to exit
                    let mut retries = 0;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(80));

                        let last_activity = *last_stdout_activity.lock().unwrap();

                        // Check if shell_pid has any child processes
                        let has_children = nix::sys::wait::waitpid(
                            nix::unistd::Pid::from_raw(-(shell_pid as i32)), // Use -pid to wait for any children
                            Some(nix::sys::wait::WaitPidFlag::WNOHANG),
                        )
                        .is_err();

                        if last_activity.elapsed().as_millis() > 150 && !has_children {
                            retries += 1;
                            // Give it a few tries to be sure activity has really stopped
                            if retries > 3 {
                                stdin_tx.send(Some(vec![0x04])).unwrap();
                                stdin_tx.send(None).unwrap();
                                break;
                            }
                        } else {
                            retries = 0;
                        }
                    }

                    return;
                }

                let mut buf = vec![0; 1024];
                loop {
                    let n = stdin.read(&mut buf).unwrap();
                    if n == 0 {
                        stdin_tx.send(None).unwrap();
                        return;
                    }
                    stdin_tx.send(Some(buf[..n].to_vec())).unwrap();
                }
            });
        }

        // read from stdin_rx and write to pty
        {
            std::thread::spawn(move || {
                for buf in stdin_rx {
                    if let Some(buf) = buf {
                        writer.write_all(&buf).unwrap();
                    } else {
                        break;
                    }
                }
            });
        }

        let res = child.wait().unwrap();
        tracing::debug!("child exited: {}", res);

        // send None to output_tx which will cause the output_rx thread to exit
        let _ = output_tx.send(None);

        // wait for the output collection thread to exit
        let (out, stdout) = collect_output_jh.join().unwrap();
        drop(stdout);

        let result = {
            // parse using anstyle_parse
            let mut parser = anstyle_parse::Parser::<anstyle_parse::Utf8Parser>::new();
            let mut performer = impls::Performer {
                strict: args.strict,
                ..Default::default()
            };

            for &byte in &out[..] {
                parser.advance(&mut performer, byte);
            }

            performer.finish(FormatAnsiStyle::Markdown)
        };

        let mut dests = vec![];

        // write result to `xclip` command if it's in PATH
        if let Ok(xclip) = which::which("xclip") {
            let mut cmd = std::process::Command::new(xclip);
            cmd.arg("-selection").arg("clipboard");
            cmd.stdin(std::process::Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(result.as_bytes())
                .unwrap();
            child.wait().unwrap();

            dests.push("📋 X11 clipboard");
        }

        // write result to `clip.exe` command if it's in PATH
        if let Ok(clip) = which::which("clip.exe") {
            // first convert result to utf-16
            let result = result.encode_utf16().collect::<Vec<_>>();

            let mut cmd = std::process::Command::new(clip);
            cmd.stdin(std::process::Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            for &c in &result {
                child
                    .stdin
                    .as_mut()
                    .unwrap()
                    .write_all(&c.to_le_bytes())
                    .unwrap();
            }
            child.wait().unwrap();

            dests.push("📋 Windows clipboard");
        }

        // write result to `pbcopy` command if it's in PATH
        if let Ok(pbcopy) = which::which("pbcopy") {
            let mut cmd = std::process::Command::new(pbcopy);
            cmd.stdin(std::process::Stdio::piped());
            let mut child = cmd.spawn().unwrap();
            child
                .stdin
                .as_mut()
                .unwrap()
                .write_all(result.as_bytes())
                .unwrap();
            child.wait().unwrap();

            dests.push("📋 macOS clipboard");
        }

        // write result to `/tmp/terminus-output` file
        fs_err::write("/tmp/terminus-output", result.as_bytes()).unwrap();
        dests.push("📂 /tmp/terminus-output");

        eprintln!("✂️ Wrote output to:");
        for dest in &dests {
            eprintln!("  - {}", dest.green());
        }
    }
}

#[cfg(feature = "impl")]
pub(crate) mod impls;
