#[cfg(debug_assertions)]
use std::cell::Cell;

#[cfg(debug_assertions)]
thread_local! {
    static VERBOSE: Cell<bool> = Cell::new(false);
}

#[cfg(debug_assertions)]
pub fn set_verbose(verbose: bool) {
    VERBOSE.with(|v| v.set(verbose))
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug_info {
    (target: $target:expr, $($arg:tt)+) => (
        if crate::verbose::verbose_mode(){
            log::info!(target: $target, $crate::Level::Error, $($arg)+)
        }
    );
    ($($arg:tt)+) => (
        if crate::verbose::verbose_mode(){
            log::info!($($arg)+)
        }
    )
}

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! debug_warn {
    (target: $target:expr, $($arg:tt)+) => (
        if crate::verbose::verbose_mode(){
            log::warn!(target: $target, $crate::Level::Error, $($arg)+)
        }
    );
    ($($arg:tt)+) => (
        if crate::verbose::verbose_mode(){
            log::warn!($($arg)+)
        }
    )
}

#[cfg(debug_assertions)]
#[inline]
pub(crate) fn verbose_mode() -> bool {
    VERBOSE.with(|v| v.get())
}

#[cfg(not(debug_assertions))]
#[inline(always)]
pub fn set_verbose(_verbose: bool) {}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug_info {
    (target: $target:expr, $($arg:tt)+) => {};
    ($($arg:tt)+) => {};
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! debug_warn {
    (target: $target:expr, $($arg:tt)+) => {};
    ($($arg:tt)+) => {};
}

#[cfg(not(debug_assertions))]
pub(crate) fn verbose_mode() -> bool {
    false
}
