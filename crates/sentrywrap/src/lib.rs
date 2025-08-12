use config_types::Environment;
pub use sentry;
use sentry::ClientInitGuard;

pub fn install() -> ClientInitGuard {
    let _guard = sentry::init((
        "https://a02afe0f91aa0f0719974fc71834a401@o1172311.ingest.us.sentry.io/4509831845707776",
        sentry::ClientOptions {
            release: sentry::release_name!(),
            // Capture user IPs and potentially sensitive headers when using HTTP server integrations
            // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
            send_default_pii: true,
            environment: Some(
                if Environment::default().is_prod() {
                    "production"
                } else {
                    "development"
                }
                .into(),
            ),
            enable_logs: true,
            attach_stacktrace: true,
            default_integrations: true,
            server_name: Some(
                hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "unknown".to_string())
                    .into(),
            ),
            ..Default::default()
        },
    ));
    sentry::capture_message("Hello World!", sentry::Level::Info);
    _guard
}

// 🐻‍❄️👀 sentry-eyre: Sentry integration for `eyre`.
// Copyright (c) 2023-2024 Noel Towa <cutie@floofy.dev>
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

use eyre::Report;
use sentry::{Hub, event_from_error, protocol::Event, types::Uuid};
use std::error::Error;

/// Captures a [`Report`] and sends it to Sentry. Refer to the top-level
/// module documentation on how to use this method.
pub fn capture_report(report: &Report) -> Uuid {
    Hub::with_active(|hub| hub.capture_report(report))
}

/// Utility function to represent a Sentry [`Event`] from a [`Report`]. This shouldn't
/// be consumed directly unless you want access to the created [`Event`] from a [`Report`].
pub fn event_from_report(report: &Report) -> Event<'static> {
    // TODO: take whole chain into account, format backtrace if given, strip ANSI codes.
    // right now it's not very useful.

    let err: &dyn Error = report.as_ref();
    event_from_error(err)
}

/// Extension trait to implement a `capture_report` method on any implementations.
pub trait CaptureReportExt: private::Sealed {
    /// Captures a [`Report`] and sends it to Sentry. Refer to the top-level
    /// module documentation on how to use this method.
    fn capture_report(&self, report: &Report) -> Uuid;
}

impl CaptureReportExt for Hub {
    fn capture_report(&self, report: &Report) -> Uuid {
        self.capture_event(event_from_report(report))
    }
}

mod private {
    pub trait Sealed {}

    impl Sealed for sentry::Hub {}
}
