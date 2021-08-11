use std::sync::atomic::{AtomicBool, Ordering};

pub mod io;
pub mod ssh;

#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr,)+) => { hashmap!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::rustc_hash::FxHashMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map.shrink_to_fit();
            _map
        }
    };
}

static STOP_SOON: AtomicBool = AtomicBool::new(false);

#[inline]
pub fn notify_stop() {
    STOP_SOON.store(true, Ordering::Relaxed);
}

#[inline]
pub fn stop_soon() -> bool {
    STOP_SOON.load(Ordering::Relaxed)
}

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

pub fn set_debug(debug: bool) {
    DEBUG_MODE.store(debug, Ordering::Relaxed)
}

#[inline]
pub fn debug() -> bool {
    DEBUG_MODE.load(Ordering::Relaxed)
}

pub(crate) fn to_boxed_str<T: AsRef<str>>(s: T) -> Box<str> {
    let t = s.as_ref();
    String::into_boxed_str(t.to_string())
}
