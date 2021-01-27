use std::future::pending;
use std::sync::{Arc, Barrier, Once};

use tokio::runtime::{Builder, Runtime};

static mut RUNTIME: Option<Runtime> = None;
static ONCE: Once = Once::new();

fn init_runtime() {
    ONCE.call_once(|| {
        // from quaff-async.
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to init tokio runtime.");
        unsafe { RUNTIME = Some(rt) }
        let barrier = Arc::new(Barrier::new(2));
        let barrier1 = Arc::clone(&barrier);
        let task = async move {
            barrier1.wait();
            pending::<()>().await
        };
        std::thread::Builder::new()
            .name("healer-bg-thread".into())
            .spawn(move || unsafe { RUNTIME.as_ref().unwrap() }.block_on(task))
            .expect("failed to spawn healer background thread");

        // A single-threaded runtime polls the inner future for the first thread
        // which invokes block_on. If the worker thread does not invoke "block_on"
        // before other threads, other threads can "steal" the execution. Here we
        // wait for the worker thread to be ready, which takes the job of polling
        // from now on.
        barrier.wait();
        log::trace!("tokio runtime initialized");
    })
}

pub fn runtime() -> &'static Runtime {
    init_runtime();
    unsafe { RUNTIME.as_ref().unwrap() }
}
