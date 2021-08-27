use std::sync::atomic::{AtomicBool, Ordering};

static STOP_SOON: AtomicBool = AtomicBool::new(false);

pub fn stop_soon() -> bool {
    STOP_SOON.load(Ordering::Release)
}

pub fn stop_req() {
    STOP_SOON.store(true, Ordering::Release)
}
