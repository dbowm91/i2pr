//! Plan 369 — the `i2pr-apphost` process entry point.
//!
//! `i2pr-appd` launches this binary as its own child with two inherited
//! anonymous pipes on file descriptors 0 and 1, sends exactly one bounded
//! launch request, and then relays managed-app protocol bytes through it. There
//! is no flag, no config file, no discovery endpoint, and no way to run this
//! process standalone and have it mean anything: without an inherited transport
//! it has nothing to talk to, and without a manager on the other end of those
//! pipes it holds no authority.
//!
//! Arguments are deliberately **not** parsed. Plan 369 §4 makes process location
//! distribution-owned and forbids a user-configurable arbitrary command; an
//! argument that could select an executable, a root, or a launch profile would
//! reopen exactly that hole. If any argument is supplied, this process refuses
//! to start rather than ignoring it silently.

use std::process::ExitCode;

use i2pr_apphost::{inherited, serve};

fn main() -> ExitCode {
    // Refuse unknown arguments instead of tolerating them: an ignored argument
    // is indistinguishable, from the outside, from an argument that was
    // understood and ignored.
    if std::env::args_os().len() > 1 {
        eprintln!("i2pr-apphost takes no arguments; it is launched by i2pr-appd");
        return ExitCode::from(2);
    }

    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("i2pr-apphost could not start its async runtime: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(serve(inherited())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Diagnostics go to stderr only. stderr is never protocol and is
            // never interpreted as control.
            eprintln!("i2pr-apphost stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
