//! Boot up and manage virtual machine

use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Mutex, Once};
use std::{fmt, thread::sleep, time::Duration};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
};

use super::ssh;
use crate::bg_task::Reader;

pub struct QemuHandle {
    qemu: Child,
    stdout: Reader,
    stderr: Reader,
    ssh_ip: String,
    ssh_port: u16,
    ssh_key_path: String,
    ssh_user: String,
}

impl Drop for QemuHandle {
    fn drop(&mut self) {
        self.kill_qemu();
    }
}

impl QemuHandle {
    pub fn ssh_addr(&self) -> String {
        format!("{}:{}", self.ssh_ip, self.ssh_port)
    }

    pub fn output(mut self) -> (Vec<u8>, Vec<u8>) {
        self.kill_qemu();

        let stdout = self.stdout.recv.recv().unwrap();
        let stderr = self.stderr.recv.recv().unwrap();
        (stdout, stderr)
    }

    fn kill_qemu(&mut self) {
        if self.qemu.kill().is_ok() {
            let _ = self.qemu.wait();
        }
    }

    pub fn is_alive(&self) -> Result<bool, std::io::Error> {
        let mut ssh_cmd = ssh::ssh_basic_cmd(
            &self.ssh_ip,
            self.ssh_port,
            &self.ssh_key_path,
            &self.ssh_user,
        );
        let status = ssh_cmd
            .arg("pwd")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        Ok(status.success())
    }
}

#[derive(Debug)]
pub enum BootError {
    Config(String),
    Spawn(std::io::Error),
    NoFreePort,
}

impl fmt::Display for BootError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootError::Config(ref err) => write!(f, "config: {}", err),
            BootError::Spawn(ref err) => write!(f, "spawn: {}", err),
            BootError::NoFreePort => write!(f, "no port to spawn qemu"),
        }
    }
}

impl From<std::io::Error> for BootError {
    fn from(err: std::io::Error) -> Self {
        BootError::Spawn(err)
    }
}

#[derive(Debug)]
pub struct QemuConf {
    pub target: String, // os/arch
    pub kernel_path: Option<Box<Path>>,
    pub img_path: Box<Path>,
    pub smp: Option<u8>,
    pub mem: Option<u8>,
    pub mem_backend_files: Vec<(Box<Path>, usize)>,
}

impl Default for QemuConf {
    fn default() -> Self {
        Self {
            target: "linux/amd64".to_string(),
            kernel_path: None,
            img_path: PathBuf::from("./test.img").into_boxed_path(),
            smp: None,
            mem: None,
            mem_backend_files: Vec::new(),
        }
    }
}

pub fn boot(conf: &QemuConf, ssh_conf: &ssh::SshConf) -> Result<QemuHandle, BootError> {
    let (mut qemu_cmd, ssh_fwd_port) = build_qemu_command(conf)?;
    qemu_cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    log::debug!("{:?}", qemu_cmd);
    let mut child = qemu_cmd.spawn()?;

    let stdout_reader = Reader::new(child.stdout.take().unwrap());
    let stderr_reader = Reader::new(child.stderr.take().unwrap());
    let ssh_user = ssh_conf
        .ssh_user
        .clone()
        .unwrap_or_else(|| String::from("root"));
    let mut qemu_handle = QemuHandle {
        qemu: child,
        stdout: stdout_reader,
        stderr: stderr_reader,
        ssh_ip: QEMU_SSH_IP.to_string(),
        ssh_port: ssh_fwd_port.0,
        ssh_key_path: ssh_conf.ssh_key.display().to_string(),
        ssh_user,
    };

    let mut wait_duration = Duration::from_millis(500);
    let min_wait_duration = Duration::from_millis(100);
    let detla = Duration::from_millis(100);
    let total = Duration::from_secs(60 * 10); // wait 10 minutes most;
    let mut waited = Duration::from_millis(0);
    let mut alive = false;
    while waited < total {
        sleep(wait_duration);
        if qemu_handle.is_alive()? {
            alive = true;
            break;
        }
        // qemu may have already exited.
        if let Some(status) = qemu_handle.qemu.try_wait()? {
            let (_, stderr) = qemu_handle.output();
            let stderr = String::from_utf8(stderr).unwrap_or_default();
            return Err(BootError::Config(format!(
                "failed to boot, qemu exited with: {}.\nSTDERR:\n{}",
                status, stderr
            )));
        }
        waited += wait_duration;
        if wait_duration > min_wait_duration {
            wait_duration -= detla;
        }
    }
    if alive {
        Ok(qemu_handle)
    } else {
        Err(BootError::Config(format!("failed to boot: {:?}", qemu_cmd)))
    }
}

const QEMU_HOST_IP: &str = "10.0.2.10";
const QEMU_SSH_IP: &str = "127.0.0.1";

fn build_qemu_command(conf: &QemuConf) -> Result<(Command, PortGuard), BootError> {
    let static_conf = static_conf(&conf.target)
        .ok_or_else(|| BootError::Config(format!("target not supported: {}", conf.target)))?;

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

    let mem = if let Some(sz) = conf.mem {
        vec!["-m".to_string(), format!("{}", sz)]
    } else {
        vec!["-m".to_string(), "1G,slots=3,maxmem=4G".to_string()]
    };

    let smp = vec!["-smp".to_string(), format!("{}", conf.smp.unwrap_or(2))];

    let ssh_fwd_port = get_free_port().ok_or(BootError::NoFreePort)?; // TODO find a free port.
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
        format!("file={},index=0,media=disk", conf.img_path.display()),
    ];

    let append = if let Some(kernel) = conf.kernel_path.as_ref() {
        let mut append = static_conf.append.clone();
        append.extend(&QEMU_LINUX_APPEND);
        vec![
            "-kernel".to_string(),
            kernel.display().to_string(),
            "-append".to_string(),
            append.join(" "),
        ]
    } else {
        Vec::new()
    };

    let mut inshm = Vec::new();
    // -device ivshmem-plain,memdev=hostmem
    // -object memory-backend-file,size={},share,mem-path={},id=hostmem
    for (i, (f, sz)) in conf.mem_backend_files.iter().enumerate() {
        let dev = vec![
            "-device".to_string(),
            format!("ivshmem-plain,memdev=hostmem{}", i),
        ];
        let obj = vec![
            "-object".to_string(),
            format!(
                "memory-backend-file,size={}M,share,mem-path={}",
                sz,
                f.display()
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
        .args(&append)
        .args(&inshm);

    Ok((qemu_cmd, ssh_fwd_port))
}

macro_rules! fxhashmap {
    ($($key:expr => $value:expr,)+) => { fxhashmap!($($key => $value),+) };
    ($($key:expr => $value:expr),*) => {
        {
            let mut _map = ::rustc_hash::FxHashMap::default();
            $(
                let _ = _map.insert($key, $value);
            )*
            _map.shrink_to_fit();
            _map
        }
    };
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
        let conf = fxhashmap! {
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
        // once can only be called once, so this is safe.
        unsafe {
            QEMU_STATIC_CONF = Some(conf);
        }
    }); // call_once
    let conf = unsafe { QEMU_STATIC_CONF.as_ref().unwrap() };
    conf.get(os_arch.as_ref())
}

static mut PORTS: Option<Mutex<FxHashSet<u16>>> = None;
static PORTS_ONCE: Once = Once::new();

fn get_free_port() -> Option<PortGuard> {
    use std::net::{Ipv4Addr, TcpListener};
    PORTS_ONCE.call_once(|| {
        unsafe { PORTS = Some(Mutex::new(FxHashSet::default())) };
    });

    let mut g = unsafe { PORTS.as_ref().unwrap().lock().unwrap() };
    for p in 1025..65535 {
        if TcpListener::bind((Ipv4Addr::LOCALHOST, p)).is_ok() {
            if g.insert(p) {
                return Some(PortGuard(p));
            }
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
