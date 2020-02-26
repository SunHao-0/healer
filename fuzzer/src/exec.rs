use crate::guest;
use crate::guest::{Crash, Guest};
use crate::utils::cli::{App, Arg, OptVal};
use crate::Config;
use core::prog::Prog;
use executor::transfer::{async_recv_result, async_send};
use executor::ExecResult;
use std::path::PathBuf;
use std::process::exit;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::oneshot;

// config for executor
#[derive(Debug, Deserialize)]
pub struct ExecutorConf {
    pub path: PathBuf,
}

pub struct Executor {
    inner: ExecutorImpl,
}

enum ExecutorImpl {
    Linux(LinuxExecutor),
}

impl Executor {
    pub fn new(cfg: &Config) -> Self {
        Self {
            inner: ExecutorImpl::Linux(LinuxExecutor::new(cfg)),
        }
    }

    pub async fn start(&mut self) {
        match self.inner {
            ExecutorImpl::Linux(ref mut e) => e.start().await,
        }
    }

    pub async fn exec(&mut self, p: &Prog) -> Result<ExecResult, Option<Crash>> {
        match self.inner {
            ExecutorImpl::Linux(ref mut e) => e.exec(p).await,
        }
    }
}

struct LinuxExecutor {
    guest: Guest,
    port: u16,
    exec_handle: Option<Child>,
    conn: Option<TcpStream>,

    executor_bin_path: PathBuf,
    target_path: PathBuf,
}

impl LinuxExecutor {
    pub fn new(cfg: &Config) -> Self {
        let guest = Guest::new(cfg);
        let port = port_check::free_local_port()
            .unwrap_or_else(|| exits!(exitcode::TEMPFAIL, "No Free port for executor driver"));

        Self {
            guest,
            port,
            exec_handle: None,
            conn: None,

            executor_bin_path: cfg.executor.path.clone(),
            target_path: PathBuf::from(&cfg.fots_bin),
        }
    }

    pub async fn start(&mut self) {
        // handle should be set to kill on drop
        self.exec_handle = None;
        self.guest.boot().await;

        self.start_executer().await
    }

    pub async fn start_executer(&mut self) {
        self.exec_handle = None;
        let target = self.guest.copy(&self.target_path).await;

        let (tx, rx) = oneshot::channel();
        let host_addr = format!("{}:{}", guest::LINUX_QEMU_HOST_IP_ADDR, self.port);
        tokio::spawn(async move {
            let mut listener = TcpListener::bind(&host_addr).await.unwrap_or_else(|e| {
                exits!(exitcode::OSERR, "Fail to listen on {}: {}", host_addr, e)
            });
            match listener.accept().await {
                Ok((conn, addr)) => {
                    info!("connected from: {}", addr);
                    tx.send(conn).unwrap();
                }
                Err(e) => {
                    eprintln!("Executor driver: fail to get client: {}", e);
                    exit(exitcode::OSERR);
                }
            }
        });

        let executor = App::new(self.executor_bin_path.to_str().unwrap())
            .arg(Arg::new_opt("-t", OptVal::normal(target.to_str().unwrap())))
            .arg(Arg::new_opt(
                "-a",
                OptVal::normal(&format!(
                    "{}:{}",
                    guest::LINUX_QEMU_USER_NET_HOST_IP_ADDR,
                    self.port
                )),
            ));

        self.exec_handle = Some(self.guest.run_cmd(&executor).await);
        self.conn = Some(rx.await.unwrap());
    }

    pub async fn exec(&mut self, p: &Prog) -> Result<ExecResult, Option<Crash>> {
        // send must be success
        assert!(self.conn.is_some());
        loop {
            async_send(p, self.conn.as_mut().unwrap()).await.unwrap();

            match async_recv_result(self.conn.as_mut().unwrap()).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    if !self.guest.is_alive().await {
                        return Err(self.guest.try_collect_crash().await);
                    } else {
                        let mut handle = self.exec_handle.take().unwrap();
                        let mut stderr = handle.stderr.take().unwrap();
                        handle.await.unwrap_or_else(|e| {
                            exits!(exitcode::OSERR, "Fail to wait executor handle:{}", e)
                        });

                        let mut err = Vec::new();
                        stderr.read_to_end(&mut err).await.unwrap();
                        warn!(
                            "Executor: Connection lost, restarting : {}:\n{}\n",
                            e,
                            String::from_utf8(err).unwrap()
                        );
                        self.start_executer().await;
                        continue;
                    }
                }
            }
        }
    }
}
