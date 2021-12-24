// //! Boot up and manage virtual machine
use crate::hashmap;
use crate::ssh;
use crate::HashMap;
use healer_io::thread::read_background;
use healer_io::BackgroundIoHandle;
use nix::unistd::setsid;
use std::os::unix::net::{UnixListener, UnixStream};
use std::{
    collections::HashSet,
    os::unix::prelude::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Mutex, Once},
    thread::sleep,
    time::{Duration, Instant},
};
use thiserror::Error;

/// Min major version of qemu
pub const MIN_QEMU_VERSION: u32 = 4;

/// Configuration of booting qemu.
#[derive(Debug, Clone)]
pub struct QemuConfig {
    /// Booting target, such as linux/amd64.
    pub target: String,
    /// Path to kernel image.
    pub kernel_img: Option<String>,
    /// Path to disk image to boot, default is "stretch.img".
    pub disk_img: String,
    /// Path to ssh secret key to login to os under test.
    pub ssh_key: String,
    /// Username to login os under test.
    pub ssh_user: String,
    /// Smp, default is 2.
    pub qemu_smp: u32,
    /// Mem size in megabyte.
    pub qemu_mem: u32,
    /// Shared memory device file path, creadted automatically if use qemu ivshm.
    pub shmids: Vec<(String, usize)>,
    /// Virt serial port, socket based
    pub serial_ports: Vec<(String, u8)>, // (path, port)
}

impl Default for QemuConfig {
    fn default() -> Self {
        Self {
            target: "linux/amd64".to_string(),
            kernel_img: Some("./bzImage".to_string()),
            disk_img: "./stretch.img".to_string(),
            ssh_key: "./stretch.id_rsa".to_string(),
            ssh_user: "root".to_string(),
            qemu_smp: 2,
            qemu_mem: 4096,
            shmids: Vec::new(),
            serial_ports: Vec::new(),
        }
    }
}

#[derive(Debug, Error)]
pub enum QemuConfigError {
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    #[error("invalid image path: {0}")]
    InvalidPath(String),
    #[error("empty ssh username")]
    EmptySshUser,
    #[error("invalid memory size '{sz}'M: {reason}")]
    InvalidMemSize { sz: usize, reason: String },
    #[error("invalid smp '{sz}': {reason}")]
    InvalidCpuNumber { sz: usize, reason: String },
    #[error("unix socket address '{0}' already in use")]
    SockAddrInUse(String),
    #[error("Bad port '{0}'")]
    BadPort(u8),
    #[error("qemu check failed: {0}")]
    QemuCheckFailed(String),
}

impl QemuConfig {
    pub fn check(&self) -> Result<(), QemuConfigError> {
        if !SUPPORTED_TARGETS.contains(&&self.target[..]) {
            return Err(QemuConfigError::UnsupportedTarget(self.target.clone()));
        }
        if !PathBuf::from(&self.disk_img).is_file() {
            return Err(QemuConfigError::InvalidPath(self.disk_img.clone()));
        }
        if let Some(kernel_img) = self.kernel_img.as_ref() {
            if !PathBuf::from(kernel_img).is_file() {
                return Err(QemuConfigError::InvalidPath(kernel_img.clone()));
            }
        }
        if !PathBuf::from(&self.ssh_key).is_file() {
            return Err(QemuConfigError::InvalidPath(self.ssh_key.clone()));
        }
        if self.ssh_user.is_empty() {
            return Err(QemuConfigError::EmptySshUser);
        }
        if self.qemu_smp == 0 || self.qemu_smp > 1024 {
            return Err(QemuConfigError::InvalidCpuNumber {
                sz: self.qemu_smp as usize,
                reason: "should be in range [1-1024]".to_string(),
            });
        }
        if self.qemu_mem <= 128 || self.qemu_mem > 1048576 {
            return Err(QemuConfigError::InvalidMemSize {
                sz: self.qemu_mem as usize,
                reason: "should be in range [128-1048576]".to_string(),
            });
        }
        for (p, port) in &self.serial_ports {
            if PathBuf::from(p).exists() {
                return Err(QemuConfigError::SockAddrInUse(p.clone()));
            }
            if *port > 30 {
                return Err(QemuConfigError::BadPort(*port));
            }
        }
        Self::check_qemu_version(&self.target)
    }

    fn check_qemu_version(target: &str) -> Result<(), QemuConfigError> {
        let qemu_conf = static_conf(target).unwrap();
        let output = Command::new(qemu_conf.qemu)
            .arg("--version")
            .output()
            .map_err(|e| {
                QemuConfigError::QemuCheckFailed(format!(
                    "failed to spawn '{}': {}",
                    qemu_conf.qemu, e
                ))
            })?;

        let cmd = format!("{} --version", qemu_conf.qemu);
        if output.status.success() {
            let output = String::from_utf8_lossy(&output.stdout);
            if let Some(version_idx) = output.find("version") {
                let start = version_idx + "version".len();
                let output = &output[start..].trim();
                // the first charactor should be majar version
                if let Some(majar) = output.chars().next() {
                    if let Some(majar) = majar.to_digit(10) {
                        if majar < MIN_QEMU_VERSION {
                            return Err(QemuConfigError::QemuCheckFailed(format!(
                                "version not match: your version '{}', required '{}'",
                                majar, MIN_QEMU_VERSION
                            )));
                        } else {
                            return Ok(());
                        }
                    }
                }
            }
            Err(QemuConfigError::QemuCheckFailed(format!(
                "failed to parse output of '{}': {}",
                cmd, output
            )))
        } else {
            Err(QemuConfigError::QemuCheckFailed(format!(
                "failed to execute '{}': {:?}",
                cmd, output.status
            )))
        }
    }

    pub fn add_shm(&mut self, shm_id: &str, sz: usize) -> &mut Self {
        let shm_path = PathBuf::from("/dev/shm").join(shm_id);
        self.shmids
            .push((shm_path.to_str().unwrap().to_owned(), sz));
        self
    }
}

pub struct QemuHandle {
    qemu_cfg: QemuConfig,
    qemu: Option<Child>,
    stdout: Option<BackgroundIoHandle>,
    stderr: Option<BackgroundIoHandle>,
    /// Forwarded port
    ssh_port: Option<u16>,
    /// Connected unix sockets
    char_dev_socks: HashMap<String, UnixStream>,
}

impl QemuHandle {
    pub fn boot(&mut self) -> Result<Duration, BootError> {
        if self.qemu.is_some() {
            log::debug!("rebooting");
            self.kill_qemu();
        }
        self.boot_inner()
    }

    pub fn addr(&self) -> Option<(String, u16)> {
        self.ssh_port.map(|port| (QEMU_SSH_IP.to_string(), port))
    }

    pub fn char_dev_sock(&mut self, path: &str) -> Option<UnixStream> {
        self.char_dev_socks.remove(path)
    }

    pub fn is_alive(&self) -> bool {
        if self.qemu.is_none() {
            return false;
        }

        let (qemu_ip, qemu_port) = self.addr().unwrap();
        let mut ssh_cmd = ssh::ssh_basic_cmd(
            &qemu_ip,
            qemu_port,
            &self.qemu_cfg.ssh_key,
            &self.qemu_cfg.ssh_user,
        );
        let output = ssh_cmd.arg("pwd").output().unwrap();
        output.status.success()
    }

    pub fn collect_crash_log(&mut self) -> Option<Vec<u8>> {
        if self.qemu.is_some() {
            let stdout = self.stdout.take().unwrap();
            let max_wait = Duration::from_secs(15); // give qemu 15s to write log
            let mut waited = Duration::new(0, 0);
            let delta = Duration::from_millis(100);
            let qemu = self.qemu.as_mut().unwrap();
            while waited < max_wait {
                if let Ok(None) = qemu.try_wait() {
                    sleep(delta);
                    waited += delta;
                } else {
                    break;
                }
            }
            self.kill_qemu(); // make sure we don't hang here
            Some(stdout.wait_finish())
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        if let Some(stdout) = self.stdout.as_mut() {
            stdout.clear_current();
        }
        if let Some(stderr) = self.stderr.as_mut() {
            stderr.clear_current();
        }
    }

    pub fn ssh(&self) -> Option<(String, String)> {
        Some((
            self.qemu_cfg.ssh_key.clone(),
            self.qemu_cfg.ssh_user.clone(),
        ))
    }
}

impl Drop for QemuHandle {
    fn drop(&mut self) {
        self.kill_qemu();
    }
}

impl QemuHandle {
    pub fn with_config(config: QemuConfig) -> Self {
        Self {
            qemu: None,
            stdout: None,
            stderr: None,
            ssh_port: None,
            qemu_cfg: config,
            char_dev_socks: HashMap::default(),
        }
    }

    fn kill_qemu(&mut self) {
        if let Some(qemu) = self.qemu.as_mut() {
            let _ = qemu.kill();
            let _ = qemu.wait();
        }
        self.qemu = None;
        self.stdout = None;
        self.stderr = None;
        self.char_dev_socks.clear();
        self.ssh_port = None;
    }

    fn boot_inner(&mut self) -> Result<Duration, BootError> {
        let (mut qemu_cmd, ssh_fwd_port) = build_qemu_command(&self.qemu_cfg);
        qemu_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            qemu_cmd.pre_exec(|| {
                let _ = setsid();
                Ok(())
            });
        }
        log::debug!("qemu cmd: {:?}", qemu_cmd);

        let mut listeners = HashMap::new();
        for (p, _) in &self.qemu_cfg.serial_ports {
            let l = UnixListener::bind(p).map_err(|e| {
                BootError::Boot(format!("failed to bind unix sort addr '{}': {}", p, e))
            })?;
            listeners.insert(p.clone(), l);
        }
        let ret = std::thread::spawn(move || {
            let mut streams = HashMap::new();
            for (p, l) in listeners {
                match l.accept() {
                    Ok((s, _)) => streams.insert(p, s),
                    Err(e) => {
                        let msg = format!(
                            "failed to accept connection from unix socket '{}': {}",
                            p, e
                        );
                        return Err(BootError::Boot(msg));
                    }
                };
            }
            Ok(streams)
        });
        let mut child = qemu_cmd.spawn()?;
        let stdout = read_background(child.stdout.take().unwrap());
        let stderr = read_background(child.stderr.take().unwrap());
        let listener_ret = ret
            .join()
            .map_err(|e| BootError::Boot(format!("failed to wait listener thread: {:?}", e)))?;
        let streams = listener_ret?;
        assert_eq!(streams.len(), self.qemu_cfg.serial_ports.len());

        *self = QemuHandle {
            qemu: Some(child),
            stdout: Some(stdout),
            stderr: Some(stderr),
            ssh_port: Some(ssh_fwd_port.0),
            qemu_cfg: self.qemu_cfg.clone(),
            char_dev_socks: streams,
        };

        let now = Instant::now();
        let mut wait_duration = Duration::from_millis(500);
        let min_wait_duration = Duration::from_millis(100);
        let delta = Duration::from_millis(100);
        let total = Duration::from_secs(60 * 10); // wait 10 minutes most;
        let mut waited = Duration::from_millis(0);
        let mut alive = false;
        let mut tries = 0;
        let mut stderr_msg = None;

        while waited < total {
            sleep(wait_duration);
            if self.is_alive() {
                self.reset();
                alive = true;
                break;
            }
            if tries % 10 == 0 {
                log::debug!("waited: {}s", waited.as_secs());
            }
            // qemu may have already exited.
            if let Some(status) = self.qemu.as_mut().unwrap().try_wait()? {
                let stderr = self.stderr.take().unwrap().wait_finish();
                let stderr = String::from_utf8_lossy(&stderr);
                stderr_msg = Some(format!(
                    "failed to boot, qemu exited with: {}\ncmdline: {:?}\nSTDERR:\n{}",
                    status, qemu_cmd, stderr
                ));
                break;
            }
            waited += wait_duration;
            if wait_duration > min_wait_duration {
                wait_duration -= delta;
            }
            tries += 1;
        }

        if alive {
            Ok(now.elapsed())
        } else {
            self.kill_qemu();
            let mut info = format!("failed to boot in {}s", waited.as_secs());
            if let Some(msg) = stderr_msg {
                info += &format!("\nstderr:\n{}", msg);
            }
            Err(BootError::Boot(info))
        }
    }
}

#[derive(Debug, Error)]
pub enum BootError {
    #[error("boot: {0}")]
    Boot(String),
    #[error("spawn: {0}")]
    Spawn(#[from] std::io::Error),
    #[error("no port to spawn qemu")]
    NoFreePort,
}

const QEMU_HOST_IP: &str = "10.0.2.10";
const QEMU_SSH_IP: &str = "127.0.0.1";
pub static SUPPORTED_TARGETS: [&str; 8] = [
    "linux/386",
    "linux/amd64",
    "linux/arm",
    "linux/arm64",
    "linux/mips64le",
    "linux/ppc64le",
    "linux/riscv64",
    "linux/s390",
];

fn build_qemu_command(conf: &QemuConfig) -> (Command, PortGuard) {
    let static_conf = static_conf(&conf.target).unwrap();

    let arch = conf.target.split('/').nth(1).unwrap();
    let mut common = vec![
        "-display",
        "none",
        "-serial",
        "stdio",
        "-no-reboot",
        "-snapshot",
    ];
    common.push("-device");
    if arch == "s390x" {
        common.push("virtio-rng-ccw");
    } else {
        common.push("virtio-rng-pci");
    }

    let arch_args = static_conf.args.split(' ').collect::<Vec<_>>();

    let mem = vec!["-m".to_string(), conf.qemu_mem.to_string()];

    let smp = vec!["-smp".to_string(), conf.qemu_smp.to_string()];

    let ssh_fwd_port = get_free_port().unwrap(); // TODO find a free port.
    let net = vec![
        "-device".to_string(),
        format!("{},netdev=net0", static_conf.net_dev),
        "-netdev".to_string(),
        format!(
            "user,id=net0,host={},hostfwd=tcp::{}-:22",
            QEMU_HOST_IP, ssh_fwd_port.0
        ),
    ];
    let image = vec![
        "-drive".to_string(),
        format!("file={},index=0,media=disk", conf.disk_img),
    ];

    let mut append = static_conf.append.clone();
    append.extend(&QEMU_LINUX_APPEND);
    let mut append_args = Vec::new();
    if let Some(kernel_img) = conf.kernel_img.as_ref() {
        append_args = vec![
            "-kernel".to_string(),
            kernel_img.clone(),
            "-append".to_string(),
            append.join(" "),
        ];
    }

    let mut inshm = Vec::new();
    for (i, (f, sz)) in conf.shmids.iter().enumerate() {
        let dev = vec![
            "-device".to_string(),
            format!("ivshmem-plain,memdev=hostmem{}", i),
        ];
        let obj = vec![
            "-object".to_string(),
            format!(
                "memory-backend-file,size={},share,mem-path={},id=hostmem{}",
                sz, f, i
            ),
        ];
        inshm.extend(dev);
        inshm.extend(obj);
    }

    // -chardev socket,path=/tmp/foo,id=foo \
    // -device virtio-serial -device virtserialport,chardev=foo,id=test0,nr=2 \
    let mut sp_devs = Vec::new();
    for (id, (p, port)) in conf.serial_ports.iter().enumerate() {
        let ch = vec![
            "-chardev".to_string(),
            format!("socket,path={},id=ch{}", p, id),
        ];
        let devs = vec![
            "-device".to_string(),
            "virtio-serial".to_string(),
            "-device".to_string(),
            format!("virtserialport,chardev=ch{},id=dev{},nr={}", id, id, port),
        ];
        sp_devs.extend(ch);
        sp_devs.extend(devs)
    }

    let mut qemu_cmd = Command::new(static_conf.qemu);
    qemu_cmd
        .args(&common)
        .args(&arch_args)
        .args(&mem)
        .args(&smp)
        .args(&net)
        .args(&image)
        .args(&append_args)
        .args(&inshm)
        .args(&sp_devs);

    (qemu_cmd, ssh_fwd_port)
}

static mut QEMU_STATIC_CONF: Option<HashMap<&str, QemuStaticConf>> = None;
static QEMU_LINUX_APPEND: [&str; 9] = [
    "earlyprintk=serial",
    "oops=panic",
    "nmi_watchdog=panic",
    "panic_on_warn=1",
    "panic=1",
    "ftrace_dump_on_oops=orig_cpu",
    "vsyscall=native",
    "net.ifnames=0",
    "biosdevname=0",
];
static ONCE: Once = Once::new();

struct QemuStaticConf {
    qemu: &'static str,
    args: &'static str,
    append: Vec<&'static str>,
    net_dev: &'static str,
}

#[macro_export]
macro_rules! hashmap {
    ($($key:expr => $value:expr,)+) => { hashmap!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = crate::HashMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map.shrink_to_fit();
            _map
        }
    };
}

fn static_conf<T: AsRef<str>>(os_arch: T) -> Option<&'static QemuStaticConf> {
    ONCE.call_once(|| {
        let conf = hashmap! {
            "linux/amd64" => QemuStaticConf{
                qemu:     "qemu-system-x86_64",
                args: "-enable-kvm -cpu host,migratable=off",
                net_dev: "e1000",
                append: vec![
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
            },
            "linux/386" => QemuStaticConf{
                qemu:   "qemu-system-i386",
                args: "",
                net_dev: "e1000",
                append: vec![
                    "root=/dev/sda",
                    "console=ttyS0",
                ],
            },
            "linux/arm64"=> QemuStaticConf{
                qemu:     "qemu-system-aarch64",
                args: "-machine virt,virtualization=on -cpu cortex-a57",
                net_dev:   "virtio-net-pci",
                append: vec![
                    "root=/dev/vda",
                    "console=ttyAMA0",
                ],
            },
            "linux/arm" => QemuStaticConf{
                qemu:   "qemu-system-arm",
                net_dev: "virtio-net-pci",
                args: "",
                append: vec![
                    "root=/dev/vda",
                    "console=ttyAMA0",
                ],
            },
            "linux/mips64le" => QemuStaticConf{
                qemu:     "qemu-system-mips64el",
                args: "-M malta -cpu MIPS64R2-generic -nodefaults",
                net_dev:   "e1000",
                append: vec![
                    "root=/dev/sda",
                    "console=ttyS0",
                ],
            },
            "linux/ppc64le" => QemuStaticConf{
                qemu:     "qemu-system-ppc64",
                args: "-enable-kvm -vga none",
                net_dev:   "virtio-net-pci",
                append:  vec![],
            },
            "linux/riscv64"=> QemuStaticConf{
                qemu:                   "qemu-system-riscv64",
                args:               "-machine virt",
                net_dev:                 "virtio-net-pci",
                append: vec![
                    "root=/dev/vda",
                    "console=ttyS0",
                ],
            },
            "linux/s390x" => QemuStaticConf{
                qemu:     "qemu-system-s390x",
                args: "-M s390-ccw-virtio -cpu max,zpci=on",
                net_dev:   "virtio-net-pci",
                append: vec![
                    "root=/dev/vda",
                ],
            },
        };
        unsafe {
            QEMU_STATIC_CONF = Some(conf);
        }
    });

    let conf = unsafe { QEMU_STATIC_CONF.as_ref().unwrap() };
    conf.get(os_arch.as_ref())
}

static mut PORTS: Option<Mutex<HashSet<u16>>> = None;
static PORTS_ONCE: Once = Once::new();

fn get_free_port() -> Option<PortGuard> {
    use std::net::{Ipv4Addr, TcpListener};
    PORTS_ONCE.call_once(|| {
        unsafe { PORTS = Some(Mutex::new(HashSet::default())) };
    });

    let mut g = unsafe { PORTS.as_ref().unwrap().lock().unwrap() };
    for p in 1025..65535 {
        if TcpListener::bind((Ipv4Addr::LOCALHOST, p)).is_ok() && g.insert(p) {
            return Some(PortGuard(p));
        }
    }
    None
}

struct PortGuard(u16);

impl Drop for PortGuard {
    fn drop(&mut self) {
        let mut g = unsafe { PORTS.as_ref().unwrap().lock().unwrap() };
        assert!(g.remove(&self.0));
    }
}
