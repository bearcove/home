use autotrait::autotrait;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
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

/// Prefixes used to filter out unwanted frames in error backtraces.
/// It ignores panic frames, test runners, and a few threading details.
const IGNORE_FRAME_PREFIXES: &[&str] = &[
    "F as axum::handler",
    "F as futures_core",
    "__pthread_cond_wait",
    "alloc::boxed::Box",
    "axum::handler",
    "axum::util",
    "color_eyre::config::EyreHook::into_eyre_hook:",
    "core::future",
    "core::ops::function",
    "core::panic",
    "core::panic::",
    "core::pin::",
    "core::result::Result",
    "eyre::",
    "futures_util::future",
    "hyper::",
    "hyper_util::",
    "std::panic",
    "std::sys::backtrace",
    "std::sys::pal",
    "std::thread::",
    "test::__rust_begin_short_backtrace",
    "test::run_test",
    "tokio::loom",
    "tokio::runtime",
    "tokio::task",
    "tower::",
];

pub fn should_include_frame_name(name: impl AsRef<str>) -> bool {
    let name = name.as_ref().trim_start_matches('<');

    if IGNORE_FRAME_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        eprintln!("[skelly] IGNORED frame: {name}");
        return false;
    }

    eprintln!("[skelly] NON-IGNORED frame: {name}");
    true
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
