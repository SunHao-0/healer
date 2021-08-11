//! Boot up and manage virtual machine
use crate::{
    hashmap,
    utils::{
        debug,
        io::{read_background, BackgroundIoHandle},
        ssh, stop_soon,
    },
    vm::ManageVm,
};

use std::{
    collections::HashSet,
    os::unix::prelude::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Mutex, Once},
    thread::sleep,
    time::{Duration, Instant},
};

use nix::unistd::setsid;
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Min major version of qemu
pub const MIN_QEMU_VERSION: u32 = 4;

/// Configuration of booting qemu.
#[derive(Debug, Clone)]
pub struct QemuConfig {
    /// Booting target, such as linux/amd64.
    pub target: String,
    /// Path to kernel image.
    pub kernel_img: Option<PathBuf>,
    /// Path to disk image to boot, default is "stretch.img".
    pub disk_img: PathBuf,
    /// Path to ssh secret key to login to os under test.
    pub ssh_key: PathBuf,
    /// Username to login os under test.
    pub ssh_user: String,
    /// Smp, default is 2.
    pub qemu_smp: u32,
    /// Mem size in megabyte.
    pub qemu_mem: u32,
    /// Shared memory device file path, creadted automatically if use qemu ivshm.
    pub shmids: Vec<(PathBuf, usize)>,
}

#[derive(Debug, Error)]
pub enum QemuConfigError {
    #[error("unsupported target: {0}")]
    UnsupportedTarget(String),
    #[error("invalid image path: {0}")]
    InvalidPath(String),
    #[error("empty ssh username")]
    EmptySshUser,
    #[error("invalid memeory size '{sz}'M: {reason}")]
    InvalidMemSize { sz: usize, reason: String },
    #[error("invalid memeory size '{sz}'M: {reason}")]
    InvalidCpuNumber { sz: usize, reason: String },
    #[error("qemu check failed: {0}")]
    QemuCheckFailed(String),
}

impl QemuConfig {
    pub fn check(&self) -> Result<(), QemuConfigError> {
        if !SUPPORTED_TARGETS.contains(&&self.target[..]) {
            return Err(QemuConfigError::UnsupportedTarget(self.target.clone()));
        }
        if !self.disk_img.is_file() {
            return Err(QemuConfigError::InvalidPath(
                self.disk_img.to_string_lossy().into_owned(),
            ));
        }

        if let Some(kernel_img) = self.kernel_img.as_ref() {
            if !kernel_img.is_file() {
                return Err(QemuConfigError::InvalidPath(
                    kernel_img.to_string_lossy().into_owned(),
                ));
            }
        }

        if !self.ssh_key.is_file() {
            return Err(QemuConfigError::InvalidPath(
                self.ssh_key.to_string_lossy().into_owned(),
            ));
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
        self.shmids.push((shm_path, sz));
        self
    }
}

pub struct QemuHandle {
    qemu_cfg: QemuConfig,

    qemu: Option<Child>,
    stdout: Option<BackgroundIoHandle>,
    stderr: Option<BackgroundIoHandle>,
    ssh_port: Option<u16>,
}

impl ManageVm for QemuHandle {
    type Error = QemuHandleError;

    fn boot(&mut self) -> Result<(), Self::Error> {
        if self.qemu.is_some() {
            log::debug!("reboot");
            self.kill_qemu();
        }
        self.boot_inner()
    }

    fn addr(&self) -> Option<(String, u16)> {
        self.ssh_port.map(|port| (QEMU_SSH_IP.to_string(), port))
    }

    fn is_alive(&mut self) -> bool {
        if self.qemu.is_none() {
            return false;
        }

        let (qemu_ip, qemu_port) = self.addr().unwrap();
        let mut ssh_cmd = ssh::ssh_basic_cmd(
            &qemu_ip,
            qemu_port,
            &self.qemu_cfg.ssh_key.to_str().unwrap().to_string(),
            &self.qemu_cfg.ssh_user,
        );
        let output = ssh_cmd.arg("pwd").output().unwrap(); // ignore this error
        let alive = output.status.success();
        if !alive && debug() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!("alive: ssh pwd: {}", stderr.trim());
        }
        alive
    }

    fn collect_crash_log(&mut self) -> Vec<u8> {
        let stdout = self.stdout.take();
        self.kill_qemu(); // make sure we don't hang here
        if let Some(stdout) = stdout {
            stdout.wait_finish()
        } else {
            Vec::new()
        }
    }

    fn reset(&mut self) -> Result<(), Self::Error> {
        if let Some(stdout) = self.stdout.as_mut() {
            if debug() {
                let stdout = stdout.current_data();
                if !stdout.is_empty() {
                    let stdout = String::from_utf8_lossy(&stdout);
                    log::debug!("qemu stdout:\n{}", stdout);
                }
            } else {
                stdout.clear_current();
            }
        }
        if let Some(stderr) = self.stderr.as_mut() {
            if debug() {
                let stderr = stderr.current_data();
                if !stderr.is_empty() {
                    let stderr = String::from_utf8_lossy(&stderr);
                    log::debug!("qemu stdout:\n{}", stderr);
                }
            } else {
                stderr.clear_current();
            }
        }
        Ok(())
    }

    fn ssh(&self) -> Option<(PathBuf, String)> {
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
    }

    fn boot_inner(&mut self) -> Result<(), QemuHandleError> {
        let (mut qemu_cmd, ssh_fwd_port) = build_qemu_command(&self.qemu_cfg);
        qemu_cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        if debug() {
            log::debug!("spawning command:\n{:?}", qemu_cmd);
        }
        unsafe {
            qemu_cmd.pre_exec(|| {
                let _ = setsid();
                Ok(())
            });
        }
        let mut child = qemu_cmd.spawn()?;
        let stdout = read_background(child.stdout.take().unwrap());
        let stderr = read_background(child.stderr.take().unwrap());

        *self = QemuHandle {
            qemu: Some(child),
            stdout: Some(stdout),
            stderr: Some(stderr),
            ssh_port: Some(ssh_fwd_port.0),
            qemu_cfg: self.qemu_cfg.clone(),
        };

        let now = Instant::now();
        let mut wait_duration = Duration::from_millis(500);
        let min_wait_duration = Duration::from_millis(100);
        let detla = Duration::from_millis(100);
        let total = Duration::from_secs(60 * 10); // wait 10 minutes most;
        let mut waited = Duration::from_millis(0);
        let mut alive = false;
        let mut tries = 0;
        while waited < total {
            if stop_soon() {
                break;
            }
            sleep(wait_duration);
            if self.is_alive() {
                alive = true;
                break;
            }
            if debug() && tries % 10 == 0 {
                log::debug!("waited: {}s", waited.as_secs());
            }

            // qemu may have already exited.
            if let Some(status) = self.qemu.as_mut().unwrap().try_wait()? {
                let stderr = self.stderr.take().unwrap().wait_finish();
                let stderr = String::from_utf8_lossy(&stderr);
                return Err(QemuHandleError::Boot(format!(
                    "failed to boot, qemu exited with: {}\ncmdline: {:?}\nSTDERR:\n{}",
                    status, qemu_cmd, stderr
                )));
            }

            waited += wait_duration;
            if wait_duration > min_wait_duration {
                wait_duration -= detla;
            }

            tries += 1;
        }

        if alive {
            log::info!("kernel booted, cost {}s", now.elapsed().as_secs());

            if debug() {
                let stdout = self.stdout.as_ref().unwrap().current_data();
                let stdout_str = String::from_utf8_lossy(&stdout);
                log::debug!("qemu boot msg:\n{}", stdout_str);
            }
            Ok(())
        } else if stop_soon() {
            Ok(())
        } else {
            self.kill_qemu();
            Err(QemuHandleError::Boot(format!(
                "failed to boot in {}s: {:?}",
                waited.as_secs(),
                qemu_cmd
            )))
        }
    }
}

#[derive(Debug, Error)]
pub enum QemuHandleError {
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
        format!("file={},index=0,media=disk", conf.disk_img.display()),
    ];

    let mut append = static_conf.append.clone();
    append.extend(&QEMU_LINUX_APPEND);
    let mut append_args = Vec::new();
    if let Some(kernel_img) = conf.kernel_img.as_ref() {
        append_args = vec![
            "-kernel".to_string(),
            kernel_img.display().to_string(),
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
                sz,
                f.display(),
                i
            ),
        ];
        inshm.extend(dev);
        inshm.extend(obj);
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
        .args(&inshm);

    (qemu_cmd, ssh_fwd_port)
}

static mut QEMU_STATIC_CONF: Option<FxHashMap<&str, QemuStaticConf>> = None;
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
