pub mod cli;
pub mod process;
pub mod queue;
pub mod split;

use std::future::Future;
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
