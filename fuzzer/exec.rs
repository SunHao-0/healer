use crate::guest;
use crate::guest::{Crash, Guest};
use crate::utils::cli::{App, Arg, OptVal};
use crate::utils::free_ipv4_port;
use crate::Config;
use core::c::to_prog;
use core::prog::Prog;
use core::target::Target;
use executor::transfer::{async_recv_result, async_send};
use executor::{ExecResult, Reason};
use std::env::temp_dir;
use std::path::PathBuf;
use std::process::exit;
use tokio::fs::write;
use tokio::io::AsyncReadExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::process::Child;
use tokio::sync::oneshot;
use tokio::time::{delay_for, timeout, Duration};

// config for executor
#[derive(Debug, Clone, Deserialize)]
pub struct ExecutorConf {
    pub path: PathBuf,
    pub host_ip: Option<String>,
    pub concurrency: bool,
    pub memleak_check: bool,
    pub script_mode: bool,
}

impl ExecutorConf {
    pub fn check(&self) {
        if !self.path.is_file() {
            eprintln!(
                "Config Error: executor executable file {} is invalid",
                self.path.display()
            );
            exit(exitcode::CONFIG)
        }

        if let Some(ip) = &self.host_ip {
            use std::net::ToSocketAddrs;
            let addr = format!("{}:8080", ip);
            if let Err(e) = addr.to_socket_addrs() {
                eprintln!(
                    "Config Error: invalid host ip `{}`: {}",
                    self.host_ip.as_ref().unwrap(),
                    e
                );
                exit(exitcode::CONFIG)
            }
        }
    }
}

pub struct Executor {
    inner: ExecutorImpl,
}

enum ExecutorImpl {
    Linux(LinuxExecutor),
    Scripy(ScriptExecutor),
}

impl Executor {
    pub fn new(cfg: &Config) -> Self {
        let inner = if cfg.executor.script_mode {
            ExecutorImpl::Scripy(ScriptExecutor::new(cfg))
        } else {
            ExecutorImpl::Linux(LinuxExecutor::new(cfg))
        };
        Self { inner }
    }

    pub async fn start(&mut self) {
        match self.inner {
            ExecutorImpl::Linux(ref mut e) => e.start().await,
            ExecutorImpl::Scripy(ref mut e) => e.start().await,
        }
    }

    pub async fn exec(&mut self, p: &Prog, t: &Target) -> Result<ExecResult, Option<Crash>> {
        match self.inner {
            ExecutorImpl::Linux(ref mut e) => e.exec(p).await,
            ExecutorImpl::Scripy(ref mut e) => e.exec(p, t).await,
        }
    }
}

struct ScriptExecutor {
    path_on_host: PathBuf,
    guest: Guest,
}

impl ScriptExecutor {
    pub fn new(cfg: &Config) -> Self {
        let guest = Guest::new(cfg);

        Self {
            path_on_host: cfg.executor.path.clone(),
            guest,
        }
    }

    pub async fn start(&mut self) {
        self.guest.boot().await;
    }

    pub async fn exec(&mut self, p: &Prog, t: &Target) -> Result<ExecResult, Option<Crash>> {
        let p_text = to_prog(p, t);
        let tmp = temp_dir().join("HEALER_test_case_v1-1-1.c");
        if let Err(e) = write(&tmp, &p_text).await {
            eprintln!(
                "Failed to write test case to tmp dir \"{}\": {}",
                tmp.display(),
                e
            );
            exit(1);
        }

        let guest_case_file = self.guest.copy(&tmp).await;
        let mut executor = App::new(self.path_on_host.to_str().unwrap());
        executor.arg(Arg::new_flag(guest_case_file.to_str().unwrap()));

        let mut exec_handle = self.guest.run_cmd(&executor).await;

        match timeout(Duration::new(15, 0), &mut exec_handle).await {
            Err(_) => Ok(ExecResult::Failed(Reason("Time out".to_string()))),
            Ok(_) => {
                let mut stdout = exec_handle.stdout.take().unwrap();
                let mut output = String::new();
                stdout.read_to_string(&mut output).await.unwrap();
                self.parse_exec_result(output).await
            }
        }
    }

    pub async fn parse_exec_result(&mut self, out: String) -> Result<ExecResult, Option<Crash>> {
        let mut result_line = String::new();

        for l in out.lines() {
            if l.contains("HEALER_EXEC_RESULT") {
                result_line = l.to_string();
            }
        }

        if !result_line.is_empty() {
            let out = out.replace(&result_line, "");
            if result_line.contains("success") {
                return Ok(ExecResult::Ok(Default::default()));
            } else if result_line.contains("failed") {
                return Ok(ExecResult::Failed(Reason(out)));
            } else if result_line.contains("crashed") {
                return Err(Some(Crash { inner: out }));
            }
        }

        if !self.guest.is_alive().await {
            Err(Some(Crash { inner: out }))
        } else {
            Ok(ExecResult::Ok(Default::default()))
        }
    }
}

struct LinuxExecutor {
    guest: Guest,
    port: u16,
    exec_handle: Option<Child>,
    conn: Option<TcpStream>,
    concurrency: bool,
    memleak_check: bool,
    executor_bin_path: PathBuf,
    target_path: PathBuf,
    host_ip: String,
}

impl LinuxExecutor {
    pub fn new(cfg: &Config) -> Self {
        let guest = Guest::new(cfg);
        let port = free_ipv4_port()
            .unwrap_or_else(|| exits!(exitcode::TEMPFAIL, "No Free port for executor driver"));
        let host_ip = cfg
            .executor
            .host_ip
            .as_ref()
            .map(String::from)
            .unwrap_or_else(|| String::from(guest::LINUX_QEMU_HOST_IP_ADDR));

        Self {
            guest,
            port,
            exec_handle: None,
            conn: None,

            concurrency: cfg.executor.concurrency,
            memleak_check: cfg.executor.memleak_check,
            executor_bin_path: cfg.executor.path.clone(),
            target_path: PathBuf::from(&cfg.fots_bin),
            host_ip,
        }
    }

    pub async fn start(&mut self) {
        // handle should be set to kill on drop
        self.exec_handle = None;
        self.guest.boot().await;

        self.start_executer().await
    }

    pub async fn start_executer(&mut self) {
        use tokio::io::ErrorKind::*;

        self.exec_handle = None;
        let target = self.guest.copy(&self.target_path).await;

        let (tx, rx) = oneshot::channel();
        let mut retry = 0;
        let mut listener;
        loop {
            let host_addr = format!("{}:{}", self.host_ip, self.port);
            listener = match TcpListener::bind(&host_addr).await {
                Ok(l) => l,
                Err(e) => {
                    if e.kind() == AddrInUse && retry != 5 {
                        self.port = free_ipv4_port().unwrap();
                        retry += 1;
                        continue;
                    } else {
                        eprintln!("Fail to listen on {}: {}", host_addr, e);
                        exit(1);
                    }
                }
            };
            break;
        }
        let host_addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            match listener.accept().await {
                Ok((conn, _addr)) => {
                    tx.send(conn).unwrap();
                }
                Err(e) => {
                    eprintln!("Executor driver: fail to get client: {}", e);
                    exit(exitcode::OSERR);
                }
            }
        });

        let mut executor = App::new(self.executor_bin_path.to_str().unwrap());
        executor
            .arg(Arg::new_opt("-t", OptVal::normal(target.to_str().unwrap())))
            .arg(Arg::new_opt(
                "-a",
                OptVal::normal(&format!(
                    "{}:{}",
                    guest::LINUX_QEMU_USER_NET_HOST_IP_ADDR,
                    self.port
                )),
            ));
        if self.memleak_check {
            executor.arg(Arg::new_flag("-m"));
        }
        if self.concurrency {
            executor.arg(Arg::new_flag("-c"));
        }

        self.exec_handle = Some(self.guest.run_cmd(&executor).await);
        self.conn = match timeout(Duration::new(32, 0), rx).await {
            Err(_) => {
                self.exec_handle = None;
                eprintln!("Time out: wait executor connection {}", host_addr);
                exit(1)
            }
            Ok(conn) => Some(conn.unwrap()),
        };
    }

    pub async fn exec(&mut self, p: &Prog) -> Result<ExecResult, Option<Crash>> {
        // send must be success
        assert!(self.conn.is_some());
        if let Err(e) = timeout(
            Duration::new(15, 0),
            async_send(p, self.conn.as_mut().unwrap()),
        )
        .await
        {
            info!("Prog send blocked: {}, restarting...", e);
            self.start().await;
            return Ok(ExecResult::Failed(Reason("Prog send blocked".into())));
        }
        // async_send(p, self.conn.as_mut().unwrap()).await.unwrap();
        let ret = {
            match timeout(
                Duration::new(15, 0),
                async_recv_result(self.conn.as_mut().unwrap()),
            )
            .await
            {
                Err(e) => {
                    info!("Prog recv blocked: {}, restarting...", e);
                    self.start().await;
                    return Ok(ExecResult::Failed(Reason("Prog send blocked".into())));
                }
                Ok(ret) => ret,
            }
        };
        match ret {
            Ok(result) => {
                self.guest.clear().await;
                if let ExecResult::Failed(ref reason) = result {
                    let rea = reason.to_string();
                    if rea.contains("CRASH-MEMLEAK") {
                        return Err(Some(Crash { inner: rea }));
                    }
                }
                return Ok(result);
            }
            Err(_) => {
                let mut crashed: bool;
                let mut retry: u8 = 0;
                loop {
                    crashed = !self.guest.is_alive().await;
                    if crashed || retry == 10 {
                        break;
                    } else {
                        retry += 1;
                        delay_for(Duration::from_millis(500)).await;
                    }
                }

                if crashed {
                    return Err(self.guest.try_collect_crash().await);
                } else {
                    let mut handle = self.exec_handle.take().unwrap();
                    let mut stdout = handle.stdout.take().unwrap();
                    let mut stderr = handle.stderr.take().unwrap();
                    handle.await.unwrap_or_else(|e| {
                        exits!(exitcode::OSERR, "Fail to wait executor handle:{}", e)
                    });

                    let mut err = Vec::new();
                    stderr.read_to_end(&mut err).await.unwrap();
                    let mut out = Vec::new();
                    stdout.read_to_end(&mut out).await.unwrap();

                    warn!(
                        "Executor: Connection lost. STDOUT:{}. STDERR: {}",
                        String::from_utf8(out).unwrap(),
                        String::from_utf8(err).unwrap()
                    );
                    self.start_executer().await;
                }
            }
        }
        // Caused by internal err
        Ok(ExecResult::Ok(Vec::new()))
    }
}
