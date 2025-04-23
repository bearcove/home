use autotrait::autotrait;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn install(&self) {
        const RUST_BACKTRACE: &str = "RUST_BACKTRACE";
        const RUST_LIB_BACKTRACE: &str = "RUST_LIB_BACKTRACE";

        // Helper to store and restore env vars
        struct EnvVarGuard<'a> {
            key: &'a str,
            old_value: Option<String>,
        }

        impl<'a> EnvVarGuard<'a> {
            fn new(key: &'a str) -> Self {
                Self {
                    key,
                    old_value: std::env::var(key).ok(),
                }
            }
        }

        impl Drop for EnvVarGuard<'_> {
            fn drop(&mut self) {
                match &self.old_value {
                    Some(val) => unsafe { std::env::set_var(self.key, val) },
                    None => unsafe { std::env::remove_var(self.key) },
                }
            }
        }

        let backtrace_guard = EnvVarGuard::new(RUST_BACKTRACE);
        let lib_backtrace_guard = EnvVarGuard::new(RUST_LIB_BACKTRACE);

        // Default settings: yes backtrace for panics, no backtrace for errors
        // Only apply defaults if neither are set yet
        if backtrace_guard.old_value.is_none() && lib_backtrace_guard.old_value.is_none() {
            unsafe {
                std::env::set_var(RUST_BACKTRACE, "1");
                std::env::set_var(RUST_LIB_BACKTRACE, "0");
            }
        }

        let result = color_eyre::config::HookBuilder::default()
            .add_frame_filter(Box::new(|frames: &mut Vec<&color_eyre::config::Frame>| {
                frames.retain(|x| x.name.as_ref().is_none_or(should_include_frame_name))
            }))
            .install();

        result.unwrap();
    }

    /// Format the backtrace with ANSI escapes. Returns None if no backtrace is available.
    fn format_backtrace_to_terminal_colors(&self, err: &eyre::Report) -> Option<String> {
        let bt = err
            .handler()
            .downcast_ref::<color_eyre::Handler>()
            .and_then(|h| h.backtrace())?;

        let mut outstream = termcolor::Buffer::ansi();
        impls::make_backtrace_printer()
            .print_trace(bt, &mut outstream)
            .unwrap();
        Some(String::from_utf8(outstream.into_inner()).unwrap())
    }
}

pub fn should_include_frame_name(name: impl AsRef<str>) -> bool {
    let name = name.as_ref();
    let excludes = ["eyre", "tokio::runtime", "core::ops::function::FnOnce"];
    !excludes.iter().any(|exclude| name.contains(exclude))
}

mod impls {
    use color_backtrace::{BacktracePrinter, Frame, Verbosity};

    pub(crate) fn make_backtrace_printer() -> BacktracePrinter {
        BacktracePrinter::new()
            .add_frame_filter(Box::new(|frames: &mut Vec<&Frame>| {
                frames.retain(|x| x.name.as_ref().is_none_or(super::should_include_frame_name))
            }))
            .lib_verbosity(Verbosity::Full)
    }
}
