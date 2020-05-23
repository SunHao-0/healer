pub mod cli;
pub mod process;
pub mod queue;
pub mod split;

use std::future::Future;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::sync::Mutex;
use tokio::sync::broadcast;

pub async fn transaction_loop<T, F, Z>(
    mut stop_signal: broadcast::Receiver<()>,
    clean_up: T,
    mut transaction_gen: F,
) where
    T: Future,
    Z: Future,
    F: FnMut() -> Z,
{
    use broadcast::TryRecvError::*;

    loop {
        match stop_signal.try_recv() {
            Ok(_) => {
                clean_up.await;
                break;
            }
            Err(Empty) => (),
            Err(e) => panic!("Unexpected braodcast receiver state: {}", e),
        }
        let transaction = transaction_gen();
        transaction.await;
    }
}

lazy_static! {
    static ref SCAN_RANGE: Mutex<(u16, u16)> = Mutex::new((1 << 12, 32768));
}

pub fn free_ipv4_port() -> Option<u16> {
    let mut r = SCAN_RANGE.lock().unwrap();
    if r.0 >= r.1 {
        return None;
    }
    let mut addr = SocketAddrV4::new(Ipv4Addr::LOCALHOST, r.0);
    for p in r.0..r.1 {
        addr.set_port(p);
        if TcpListener::bind(&addr).is_ok() {
            r.0 = p + 1;
            return Some(p);
        }
    }
    r.0 = r.1;
    None
}
