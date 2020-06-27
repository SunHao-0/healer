/// Driver for kernel to be tested
use crate::utils::cli::{App, Arg, OptVal};
use crate::utils::free_ipv4_port;
use crate::Config;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use os_pipe::{pipe, PipeReader, PipeWriter};
use std::collections::HashMap;
use std::fmt;
use std::io::{ErrorKind, Read};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::exit;
use tokio::process::Child;
use tokio::time::{delay_for, timeout, Duration};

lazy_static! {
    static ref QEMUS: HashMap<String, App> = {
        let mut qemus = HashMap::new();

        let arg_common = vec![
            Arg::new_flag("-no-reboot"),
            Arg::new_opt("-display", OptVal::normal("none")),
            Arg::new_opt("-serial", OptVal::normal("stdio")),
            Arg::new_flag("-snapshot"),
        ];

        let mut linux_amd64 = App::new("qemu-system-x86_64");
        linux_amd64
            .arg(Arg::new_flag("-enable-kvm"))
            .args(arg_common.iter())
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
                OptVal::multiple(
                    vec![
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
                    ],
                    Some(' '),
                ),
            ));

        let mut linux_arm = App::new("qemu-system-arm");
        linux_arm
            .args(arg_common.iter())
            .arg(Arg::new_opt("-net", OptVal::normal("nic")))
            .arg(Arg::new_opt(
                "-append",
                OptVal::multiple(vec!["root=/dev/vda", "console=ttyAMA0"], Some(' ')),
            ));

        let mut linux_arm64 = App::new("qemu-system-aarch64");
        linux_arm64
            .args(arg_common.iter())
            .arg(Arg::new_opt(
                "-machine",
                OptVal::multiple(vec!["virt", "virtualization=on"], Some(',')),
            ))
            .arg(Arg::new_opt("-cpu", OptVal::normal("cortex-a57")))
            .arg(Arg::new_opt("-net", OptVal::normal("nic")))
            .arg(Arg::new_opt(
                "-append",
                OptVal::multiple(vec!["root=/dev/vda", "console=ttyAMA0"], Some(' ')),
            ));

        qemus.insert("linux/amd64".to_string(), linux_amd64);
        qemus.insert("linux/arm".to_string(), linux_arm);
        qemus.insert("linux/arm64".to_string(), linux_arm64);
        qemus
    };
    pub static ref SSH: App = {
        let mut ssh = App::new("ssh");
        ssh.arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
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
            .arg(Arg::new_opt("-o", OptVal::normal("ConnectTimeout=10s")));
        ssh
    };
    pub static ref SCP: App = {
        let mut scp = App::new("scp");
        scp.arg(Arg::new_opt("-F", OptVal::normal("/dev/null")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("UserKnownHostsFile=/dev/null"),
            ))
            .arg(Arg::new_opt("-o", OptVal::normal("BatchMode=yes")))
            .arg(Arg::new_opt("-o", OptVal::normal("IdentitiesOnly=yes")))
            .arg(Arg::new_opt(
                "-o",
                OptVal::normal("StrictHostKeyChecking=no"),
            ));
        scp
    };
}

#[derive(Debug, Clone, Deserialize)]
pub struct GuestConf {
    /// Kernel to be tested
    pub os: String,
    /// Arch of build kernel
    pub arch: String,
    /// Platform to run kernel, qemu or real env
    pub platform: String,
}

pub const PLATFORM: [&str; 1] = ["qemu"];
pub const ARCH: [&str; 1] = ["amd64"];
pub const OS: [&str; 1] = ["linux"];

impl GuestConf {
    pub fn check(&self) {
        if !PLATFORM.contains(&self.platform.as_str())
            || !ARCH.contains(&self.arch.as_str())
            || !OS.contains(&self.os.as_str())
        {
            eprintln!(
                "Config Error: unsupported guest: {:?}",
                (&self.platform, &self.arch, &self.os)
            );
            exit(exitcode::CONFIG)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct QemuConf {
    pub cpu_num: u32,
    pub mem_size: u32,
    pub image: String,
    pub kernel: String,
    pub wait_boot_time: Option<u8>,
}

impl QemuConf {
    pub fn check(&self) {
        let cpu_num = num_cpus::get() as u32;
        if self.cpu_num > cpu_num * 8 || self.cpu_num == 0 {
            eprintln!(
                "Config Error: invalid cpu num {}, cpu num must between (0, {}] on your system",
                self.cpu_num,
                cpu_num * 8
            );
            exit(exitcode::CONFIG)
        }

        if self.mem_size < 512 {
            eprintln!(
                "Config Error: invalid mem size {}, mem size must bigger than 512 bytes",
                self.mem_size
            );
            exit(exitcode::CONFIG)
        }
        let image = PathBuf::from(&self.image);
        let kernel = PathBuf::from(&self.kernel);
        if !image.is_file() {
            eprintln!("Config Error: image {} is invalid", self.image);
            exit(exitcode::CONFIG)
        }

        if !kernel.is_file() {
            eprintln!("Config Error: kernel {} is invalid", self.kernel);
            exit(exitcode::CONFIG)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SSHConf {
    pub key_path: String,
}

impl SSHConf {
    pub fn check(&self) {
        let key = PathBuf::from(&self.key_path);
        if !key.is_file() {
            eprintln!("Config Error: ssh key file {} is invalid", self.key_path);
            exit(exitcode::CONFIG)
        }
    }
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
    pub inner: String,
}

impl Default for Crash {
    fn default() -> Self {
        Crash {
            inner: String::new(),
        }
    }
}

impl fmt::Display for Crash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.inner)
    }
}

pub const LINUX_QEMU_HOST_IP_ADDR: &str = "localhost";
pub const LINUX_QEMU_USER_NET_HOST_IP_ADDR: &str = "10.0.2.10";
pub const LINUX_QEMU_HOST_USER: &str = "root";
pub const LINUX_QEMU_PIPE_LEN: i32 = 1024 * 1024;

pub struct LinuxQemu {
    handle: Option<Child>,
    rp: Option<PipeReader>,

    wait_boot_time: u8,
    addr: String,
    port: u16,
    key: String,
    user: String,
    guest: GuestConf,
    qemu: QemuConf,
}

impl LinuxQemu {
    pub fn new(cfg: &Config) -> Self {
        assert_eq!(cfg.guest.os, "linux");

        Self {
            handle: Option::None,
            rp: Option::None,
            wait_boot_time: cfg.qemu.wait_boot_time.unwrap_or(15),
            addr: LINUX_QEMU_HOST_IP_ADDR.to_string(),
            port: 0,
            key: cfg.ssh.key_path.clone(),
            user: LINUX_QEMU_HOST_USER.to_string(),
            guest: cfg.guest.clone(),
            qemu: cfg.qemu.clone(),
        }
    }
}

impl LinuxQemu {
    async fn boot(&mut self) {
        if let Some(ref mut h) = self.handle {
            h.kill()
                .unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to kill running guest:{}", e));
            self.rp = None;
        }

        const MAX_RETRY: u8 = 5;
        let mut retry = 0;
        loop {
            let (qemu, port) = build_qemu_cli(&self.guest, &self.qemu);
            self.port = port;

            let (mut handle, mut rp) = {
                let mut cmd = qemu.clone().into_cmd();
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

            let mut wait = 1;
            let mut started = false;
            let mut failed_reason = String::new();
            loop {
                delay_for(Duration::new(self.wait_boot_time as u64, 0)).await;

                if self.is_alive().await {
                    started = true;
                    break;
                }

                if wait == MAX_RETRY {
                    handle.kill().unwrap_or_else(|e| {
                        exits!(exitcode::OSERR, "Fail to kill failed guest:{}", e)
                    });
                    failed_reason = String::from_utf8_lossy(&read_all_nonblock(&mut rp))
                        .to_owned()
                        .to_string();
                    break;
                }
                wait += 1;
            }

            if !started {
                if !failed_reason.contains("ould not set up host forwarding rule")
                    || retry == MAX_RETRY
                {
                    eprintln!("Fail to boot kernel:");
                    eprintln!("{}", failed_reason);
                    eprintln!("======================= Command ===========================");
                    eprintln!("{:?}", qemu);
                    exit(1)
                } else {
                    retry += 1
                }
            } else {
                // clear useless data in pipe
                read_all_nonblock(&mut rp);
                self.handle = Some(handle);
                self.rp = Some(rp);
                break;
            }
        }
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

        let mut scp = SCP.clone();
        scp.arg(Arg::new_opt("-P", OptVal::normal(&self.port.to_string())))
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
        match timeout(Duration::new(30, 0), self.handle.as_mut().unwrap()).await {
            Err(_e) => {
                if !self.is_alive().await {
                    Some(self.collect_crash())
                } else {
                    None
                }
            }
            Ok(_) => Some(self.collect_crash()),
        }
    }

    fn collect_crash(&mut self) -> Crash {
        self.handle = None;
        let crash = read_all_nonblock(self.rp.as_mut().unwrap());
        let crash_info = String::from_utf8_lossy(&crash).to_string();
        self.rp = None;
        Crash { inner: crash_info }
    }
}

fn build_qemu_cli(g: &GuestConf, q: &QemuConf) -> (App, u16) {
    let target = format!("{}/{}", g.os, g.arch);

    let mut qemu = QEMUS
        .get(&target)
        .unwrap_or_else(|| exits!(exitcode::CONFIG, "Unsupported target:{}", &target))
        .clone();

    // use low level port
    let port =
        free_ipv4_port().unwrap_or_else(|| exits!(exitcode::TEMPFAIL, "No Free port to forword"));
    let cfg = q;

    qemu.arg(Arg::new_opt("-m", OptVal::Normal(cfg.mem_size.to_string())))
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
    let mut ssh = SSH.clone();
    ssh.arg(Arg::new_opt("-p", OptVal::normal(&port.to_string())))
        .arg(Arg::new_opt("-i", OptVal::normal(key)))
        .arg(Arg::Flag(format!("{}@{}", user, addr)))
        .arg(Arg::new_flag(&app.bin));
    for app_arg in app.iter_arg() {
        ssh.arg(Arg::Flag(app_arg));
    }
    ssh
}

#[allow(unused)]
fn long_pipe() -> (PipeReader, PipeWriter) {
    let (rp, wp) = pipe().unwrap_or_else(|e| exits!(exitcode::OSERR, "Fail to creat pipe:{}", e));

    let mut sz = 128 << 10;
    while sz <= 2 << 20 {
        fcntl(wp.as_raw_fd(), FcntlArg::F_SETPIPE_SZ(sz));
        sz *= 2;
    }

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
