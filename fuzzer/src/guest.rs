/// Driver for kernel to be tested
use crate::utils::cli::{App, Arg, OptVal};
use crate::Config;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use os_pipe::{pipe, PipeReader, PipeWriter};
use serde::export::Formatter;
use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use tokio::process::Child;
use tokio::time::{delay_for, timeout, Duration};

lazy_static! {
    static ref QEMUS: HashMap<String, App> = {
        let mut qemus = HashMap::new();
        let linux_amd64_append_vals = vec![
            "earlyprintk=serial",
            "oops=panic",
            "nmi_watchdog=panic",
            "panic_on_warn=1",
            "panic=1",
            "ftrace_dump_on_oops=orig_cpu",
            "rodata=n",
            "vsyscall=native",
            "net.ifnames=0",
            "biosdevname=0",
            "root=/dev/sda",
            "console=ttyS0",
            "kvm-intel.nested=1",
            "kvm-intel.unrestricted_guest=1",
            "kvm-intel.vmm_exclusive=1",
            "kvm-intel.fasteoi=1",
            "kvm-intel.ept=1",
            "kvm-intel.flexpriority=1",
            "kvm-intel.vpid=1",
            "kvm-intel.emulate_invalid_guest_state=1",
            "kvm-intel.eptad=1",
            "kvm-intel.enable_shadow_vmcs=1",
            "kvm-intel.pml=1",
            "kvm-intel.enable_apicv=1",
        ];
        let linux_amd64 = App::new("qemu-system-x86_64")
            .arg(Arg::new_flag("-enable-kvm"))
            .arg(Arg::new_flag("-no-reboot"))
            .arg(Arg::new_opt("-display", OptVal::normal("none")))
            .arg(Arg::new_opt("-serial", OptVal::normal("stdio")))
            .arg(Arg::new_flag("-snapshot"))
            .arg(Arg::new_opt(
                "-cpu",
                OptVal::multiple(vec!["host", "migratable=off"], Some(',')),
            ))
            .arg(Arg::new_opt(
                "-net",
                OptVal::multiple(vec!["nic", "model=e1000"], Some(',')),
            ))
            .arg(Arg::new_opt(
                "-append",
                OptVal::multiple(linux_amd64_append_vals, Some(' ')),
            ));
        qemus.insert("linux/amd64".to_string(), linux_amd64);

        qemus
    };
    pub static ref SSH: App = {
        App::new("ssh")
            .arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("UserKnownHostsFile=/dev/null"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("BatchMode=yes")))
            .arg(Arg::new_opt("-o", OptVal::normal("IdentitiesOnly=yes")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("StrictHostKeyChecking=no"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("ConnectTimeout=10s")))
    };
    pub static ref SCP: App = {
        App::new("scp")
            .arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("UserKnownHostsFile=/dev/null"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("BatchMode=yes")))
            .arg(Arg::new_opt("-o", OptVal::normal("IdentitiesOnly=yes")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("StrictHostKeyChecking=no"),
            ))
    };
}

#[derive(Debug, Deserialize)]
pub struct GuestConf {
    /// Kernel to be tested
    pub os: String,
    /// Arch of build kernel
    pub arch: String,
    /// Platform to run kernel, qemu or real env
    pub platform: String,
}

#[derive(Debug, Deserialize)]
pub struct QemuConf {
    pub cpu_num: u32,
    pub mem_size: u32,
    pub image: String,
    pub kernel: String,
    pub wait_boot_time: Option<u8>,
}

#[derive(Debug, Deserialize)]
pub struct SSHConf {
    pub key_path: String,
}

pub enum Guest {
    LinuxQemu(LinuxQemu),
}

impl Guest {
    pub fn new(cfg: &Config) -> Self {
        // only support linux/amd64 on qemu now.
        Guest::LinuxQemu(LinuxQemu::new(cfg))
    }
}

impl Guest {
    /// Boot guest or panic
    pub async fn boot(&mut self) {
        match self {
            Guest::LinuxQemu(ref mut guest) => guest.boot().await,
        }
    }

    /// Judge if guest is  still alive
    pub async fn is_alive(&self) -> bool {
        match self {
            Guest::LinuxQemu(ref guest) => guest.is_alive().await,
        }
    }

    /// Run command on guest,return handle or crash
    pub async fn run_cmd(&self, app: &App) -> Child {
        match self {
            Guest::LinuxQemu(ref guest) => guest.run_cmd(app).await,
        }
    }

    /// Try collect crash info guest, this could be none sometimes
    pub async fn try_collect_crash(&mut self) -> Option<Crash> {
        match self {
            Guest::LinuxQemu(ref mut guest) => guest.try_collect_crash().await,
        }
    }

    pub async fn clear(&mut self) {
        match self {
            Guest::LinuxQemu(ref mut guest) => guest.clear().await,
        }
    }

    /// Copy file from host to guest, return path in guest or crash
    pub async fn copy<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        match self {
            Guest::LinuxQemu(ref guest) => guest.copy(path).await,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Crash {
    inner: String,
}

impl Default for Crash {
    fn default() -> Self {
        Crash {
            inner: String::from("$$"),
        }
    }
}

impl fmt::Display for Crash {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

pub const LINUX_QEMU_HOST_IP_ADDR: &str = "localhost";
pub const LINUX_QEMU_USER_NET_HOST_IP_ADDR: &str = "10.0.2.10";
pub const LINUX_QEMU_HOST_USER: &str = "root";
pub const LINUX_QEMU_PIPE_LEN: i32 = 1024 * 1024;

pub struct LinuxQemu {
    vm: App,
    wait_boot_time: u8,
    handle: Option<Child>,
    rp: Option<PipeReader>,

    addr: String,
    port: u16,
    key: String,
    user: String,
}

impl LinuxQemu {
    pub fn new(cfg: &Config) -> Self {
        assert_eq!(cfg.guest.platform.trim(), "qemu");
        assert_eq!(cfg.guest.os, "linux");
        assert_eq!(cfg.guest.arch, "amd64");

        let (qemu, port) = build_qemu_cli(&cfg);
        let ssh_conf = cfg
            .ssh
            .as_ref()
            .unwrap_or_else(|| exits!(exitcode::CONFIG, "Require ssh segment in config toml"));

        Self {
            vm: qemu,
            handle: None,
            rp: None,

            wait_boot_time: cfg.qemu.as_ref().unwrap().wait_boot_time.unwrap_or(5),
            addr: LINUX_QEMU_HOST_IP_ADDR.to_string(),
            port,
            key: ssh_conf.key_path.clone(),
            user: LINUX_QEMU_HOST_USER.to_string(),
        }
    }
}

impl LinuxQemu {
    async fn boot(&mut self) {
        const MAX_RETRY: u8 = 5;

        if let Some(ref mut h) = self.handle {
            h.kill()
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to kill running guest:{}", e));
            self.rp = None;
        }

        let (mut handle, mut rp) = {
            let mut cmd = self.vm.clone().into_cmd();
            let (rp, wp) = long_pipe();
            fcntl(rp.as_raw_fd(), FcntlArg::F_SETFL(OFlag::O_NONBLOCK))
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to set flag on pipe:{}", e));
            let wp2 = wp
                .try_clone()
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to clone pipe:{}", e));
            let handle = cmd
                .stdin(std::process::Stdio::piped())
                .stdout(wp)
                .stderr(wp2)
                .kill_on_drop(true)
                .spawn()
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to spawn qemu:{}", e));

            (handle, rp)
        };

        let mut retry = 1;
        loop {
            delay_for(Duration::new(self.wait_boot_time as u64, 0)).await;

            if self.is_alive().await {
                break;
            }

            if retry == MAX_RETRY {
                handle
                    .kill()
                    .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to kill failed guest:{}", e));
                let mut buf = String::new();
                rp.read_to_string(&mut buf).unwrap_or_else(|e| {
                    exits!(exitcode::OSERR, "Fail to read to end of pipe:{}", e)
                });
                eprintln!("{}", buf);
                eprintln!("===============================================");
                exits!(exitcode::DATAERR, "Fail to boot :\n{:?}", self.vm);
            }
            retry += 1;
        }
        // clear useless data in pipe
        read_all_nonblock(&mut rp);
        self.handle = Some(handle);
        self.rp = Some(rp);
    }

    async fn is_alive(&self) -> bool {
        let mut pwd = ssh_app(
            &self.key,
            &self.user,
            &self.addr,
            self.port,
            App::new("pwd"),
        )
        .into_cmd();
        pwd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        match timeout(Duration::new(10, 0), pwd.status()).await {
            Err(_) => false,
            Ok(status) => match status {
                Ok(status) => status.success(),
                Err(e) => exits!(exitcode::OSERR, "Fail to spawn detector(ssh:pwd):{}", e),
            },
        }
    }

    async fn run_cmd(&self, app: &App) -> Child {
        assert!(self.handle.is_some());

        let mut app = app.clone();
        let bin = self.copy(PathBuf::from(&app.bin)).await;
        app.bin = String::from(bin.to_str().unwrap());
        let mut app = ssh_app(&self.key, &self.user, &self.addr, self.port, app).into_cmd();
        app.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to spawn:{}", e))
    }

    async fn clear(&mut self) {
        if let Some(r) = self.rp.as_mut() {
            read_all_nonblock(r);
        }
    }

    pub async fn copy<T: AsRef<Path>>(&self, path: T) -> PathBuf {
        let path = path.as_ref();
        assert!(path.is_file());

        let file_name = path.file_name().unwrap().to_str().unwrap();
        let guest_path = PathBuf::from(format!("~/{}", file_name));

        let scp = SCP
            .clone()
            .arg(Arg::new_opt("-P", OptVal::normal(&self.port.to_string())))
            .arg(Arg::new_opt("-i", OptVal::normal(&self.key)))
            .arg(Arg::new_flag(path.to_str().unwrap()))
            .arg(Arg::Flag(format!(
                "{}@{}:{}",
                self.user,
                self.addr,
                guest_path.display()
            )));

        let output = scp
            .into_cmd()
            .output()
            .await
            .unwrap_or_else(|e| panic!("Failed to spawn:{}", e));

        if !output.status.success() {
            panic!(String::from_utf8(output.stderr).unwrap());
        }
        guest_path
    }

    async fn try_collect_crash(&mut self) -> Option<Crash> {
        assert!(self.rp.is_some());
        match timeout(Duration::new(2, 0), self.handle.as_mut().unwrap()).await {
            Err(_e) => None,
            Ok(_) => {
                self.handle = None;
                let crash = read_all_nonblock(self.rp.as_mut().unwrap());
                let crash_info = String::from_utf8_lossy(&crash).to_string();
                self.rp = None;
                Some(Crash { inner: crash_info })
            }
        }
    }
}

fn build_qemu_cli(cfg: &Config) -> (App, u16) {
    let target = format!("{}/{}", cfg.guest.os, cfg.guest.arch);

    let default_qemu = QEMUS
        .get(&target)
        .unwrap_or_else(|| exits!(exitcode::CONFIG, "Unsupported target:{}", &target))
        .clone();

    let port = port_check::free_local_port()
        .unwrap_or_else(|| exits!(exitcode::TEMPFAIL, "No Free port to forword"));
    let cfg = &cfg
        .qemu
        .as_ref()
        .unwrap_or_else(|| exits!(exitcode::SOFTWARE, "Require qemu segment in config toml"));
    let qemu = default_qemu
        .arg(Arg::new_opt("-m", OptVal::Normal(cfg.mem_size.to_string())))
        .arg(Arg::new_opt(
            "-smp",
            OptVal::Normal(cfg.cpu_num.to_string()),
        ))
        .arg(Arg::new_opt(
            "-net",
            OptVal::Multiple {
                vals: vec![
                    String::from("user"),
                    format!("host={}", LINUX_QEMU_USER_NET_HOST_IP_ADDR),
                    format!("hostfwd=tcp::{}-:22", port),
                ],
                sp: Some(','),
            },
        ))
        .arg(Arg::new_opt("-hda", OptVal::Normal(cfg.image.clone())))
        .arg(Arg::new_opt("-kernel", OptVal::Normal(cfg.kernel.clone())));
    (qemu, port)
}

fn ssh_app(key: &str, user: &str, addr: &str, port: u16, app: App) -> App {
    let mut ssh = SSH
        .clone()
        .arg(Arg::new_opt("-p", OptVal::normal(&port.to_string())))
        .arg(Arg::new_opt("-i", OptVal::normal(key)))
        .arg(Arg::Flag(format!("{}@{}", user, addr)))
        .arg(Arg::new_flag(&app.bin));
    for app_arg in app.iter_arg() {
        ssh = ssh.arg(Arg::Flag(app_arg));
    }
    ssh
}

fn long_pipe() -> (PipeReader, PipeWriter) {
    let (rp, wp) = pipe().unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to creat pipe:{}", e));
    fcntl(wp.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(1024 * 1024)).unwrap_or_else(|e| {
        exits!(
            exitcode::OSERR,
            "Fail to set pipe size to {} :{}",
            1024 * 1024,
            e
        )
    });

    (rp, wp)
}

fn read_all_nonblock(rp: &mut PipeReader) -> Vec<u8> {
    const BUF_LEN: usize = 1024 * 1024;
    let mut result = Vec::with_capacity(BUF_LEN);
    unsafe {
        result.set_len(BUF_LEN);
    }
    match rp.read(&mut result[..]) {
        Ok(n) => unsafe {
            result.set_len(n);
        },
        Err(e) => match e.kind() {
            ErrorKind::WouldBlock => (),
            _ => panic!(e),
        },
    }
    result.shrink_to_fit();
    result
}
