//! Signal interruption handling for graceful termination and process tree cleanup.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
static SIGNAL_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Installs OS signal handlers for SIGINT (Ctrl+C) and SIGTERM.
pub fn install_signal_handlers() -> Result<(), std::io::Error> {
    use signal_hook::iterator::Signals;
    let mut signals = Signals::new([signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM])?;

    std::thread::Builder::new()
        .name("unarc-signals".into())
        .spawn(move || {
            for _sig in signals.forever() {
                let count = SIGNAL_COUNT.fetch_add(1, Ordering::SeqCst);
                if count >= 2 {
                    // Force terminate on repeated interruption signals
                    #[cfg(unix)]
                    unsafe {
                        libc::_exit(130);
                    }
                    #[cfg(not(unix))]
                    std::process::exit(130);
                }

                INTERRUPTED.store(true, Ordering::SeqCst);
                crate::security::sandbox::terminate_active_subprocess();
            }
        })?;

    Ok(())
}

/// Checks whether an interruption signal was received.
#[must_use]
pub fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// Resets the interrupted flag (used when recovering from interruption in interactive mode).
pub fn reset_interrupted() {
    INTERRUPTED.store(false, Ordering::SeqCst);
    SIGNAL_COUNT.store(0, Ordering::SeqCst);
}

/// Manually sets the interrupted flag (primarily used for deterministic testing).
pub fn set_interrupted(val: bool) {
    INTERRUPTED.store(val, Ordering::SeqCst);
}

/// Returns `Err(UnarcError::Interrupted)` if an interruption signal was received.
pub fn check_interrupted() -> Result<(), crate::error::UnarcError> {
    if is_interrupted() {
        Err(crate::error::UnarcError::Interrupted)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interrupted_flag_lifecycle() {
        reset_interrupted();
        assert!(!is_interrupted());
        assert!(check_interrupted().is_ok());

        set_interrupted(true);
        assert!(is_interrupted());
        assert!(matches!(
            check_interrupted(),
            Err(crate::error::UnarcError::Interrupted)
        ));

        reset_interrupted();
        assert!(!is_interrupted());
        assert!(check_interrupted().is_ok());
    }
}
