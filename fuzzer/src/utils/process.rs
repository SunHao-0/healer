//! Startint long-term process, capture its stdin/stdout/stderr

use crate::utils::cli::App;
use bytes::BytesMut;
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncRead, AsyncReadExt, Result};
use tokio::process::Child;
use tokio::sync::{mpsc, oneshot};
use tokio::time::Duration;

pub struct Handle {
    timeout: Option<oneshot::Receiver<()>>,
    kill: oneshot::Sender<()>,
    done: oneshot::Receiver<Result<ExitStatus>>,

    finished: Option<Reason>,

    pub stdout: mpsc::UnboundedReceiver<BytesMut>,
    pub stderr: mpsc::UnboundedReceiver<BytesMut>,
    pub stdin: tokio::process::ChildStdin,
}

#[derive(Debug)]
pub enum Reason {
    Timeout,
    Done(Option<ExitStatus>),
    Killed,
}

impl Handle {
    pub fn kill(self) {
        if !self.kill.is_closed() {
            self.kill.send(()).unwrap()
        }
    }

    pub fn check_finish_signal(&mut self) -> Option<Reason> {
        if self.finished.is_some() {
            return self.finished.clone();
        }

        if let Some(t) = self.timeout.as_mut() {
            if t.try_recv().is_ok() {
                self.finished = Some(Reason::Timeout);
            }
        }
        if let Ok(status) = self.done.try_recv() {
            self.finished = Some(Reason::Done(status.ok()));
        }
        self.finished.clone()
    }
}

pub fn spawn(app: App, timeout: Option<Duration>) -> Handle {
    let bin = app.bin.clone();
    let mut cmd = app.into_cmd();
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut handle = cmd
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to spawn {}:{}", bin, e));

    // The length of pipe is too small, redirect it to unbounded channel
    let stdin = handle.stdin.take().unwrap();
    let stdout = redirect(handle.stdout.take().unwrap());
    let stderr = redirect(handle.stderr.take().unwrap());

    let (timeout, kill, done) = monitor(handle, timeout);
    Handle {
        timeout,
        kill,
        done,
        finished: None,
        stdout,
        stderr,
        stdin,
    }
}

fn redirect<T: AsyncRead + Send + Sync + 'static>(mut src: T) -> mpsc::UnboundedReceiver<BytesMut> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let mut buf = BytesMut::with_capacity(1024);
            if src.read_buf(&mut buf).await.is_ok() {
                buf.truncate(buf.len());
                if tx.send(buf).is_err() {
                    break;
                }
            } else {
                break;
            }
        }
    });
    rx
}

fn monitor(
    handle: Child,
    timeout: Option<Duration>,
) -> (
    Option<oneshot::Receiver<()>>,
    oneshot::Sender<()>,
    oneshot::Receiver<Result<ExitStatus>>,
) {
    let (done_tx, done_rx) = oneshot::channel();
    let (kill_tx, kill_rx) = oneshot::channel();
    let (timeout_tx, timeout_rx) = oneshot::channel();
    let timeout_rx = if timeout.is_some() {
        Some(timeout_rx)
    } else {
        None
    };

    tokio::spawn(async move {
        if let Some(duration) = timeout {
            if tokio::time::timeout(duration, kill_or_done(kill_rx, handle, done_tx))
                .await
                .is_err()
            {
                timeout_tx.send(()).unwrap()
            }
        } else {
            kill_or_done(kill_rx, handle, done_tx).await
        }
    });

    (timeout_rx, kill_tx, done_rx)
}

async fn kill_or_done(
    kill_rx: oneshot::Receiver<()>,
    handle: Child,
    done_tx: oneshot::Sender<Result<ExitStatus>>,
) {
    tokio::select! {
        msg = kill_rx => {
            if let Err(e) =  msg {
                panic!(e)
            }
        }
        status =  handle => {
            done_tx.send(status).unwrap();
        }
    }
}
