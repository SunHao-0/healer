use std::cell::Cell;

thread_local! {
    static FUZZER_ID: Cell<u64> = Cell::new(0);
}

#[inline]
pub fn set_fuzzer_id(id: u64) {
    FUZZER_ID.with(|r| r.set(id));
}

#[inline]
pub fn fuzzer_id() -> u64 {
    FUZZER_ID.with(|r| r.get())
}

#[macro_export]
macro_rules! fuzzer_trace {
    ($t: tt, $($arg:tt)*) => (
        log::trace!(std::concat!("fuzzer-{}: ", $t), crate::fuzzer_log::fuzzer_id(), $($arg)*)
    )
}

#[macro_export]
macro_rules! fuzzer_info {
    ($t: tt, $($arg:tt)*) => (
        log::info!(std::concat!("fuzzer-{}: ", $t), crate::fuzzer_log::fuzzer_id(), $($arg)*)
    )
}

#[macro_export]
macro_rules! fuzzer_warn {
    ($t: tt, $($arg:tt)*) => (
        log::warn!(std::concat!("fuzzer-{}: ", $t), crate::fuzzer_log::fuzzer_id(), $($arg)*)
    )
}
