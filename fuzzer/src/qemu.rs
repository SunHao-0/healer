use crate::ssh::ssh_run;
use crate::utils::cli::{App, Arg, OptVal};
use bytes::BytesMut;
use std::collections::HashMap;
use std::process::{ExitStatus, Stdio};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::Child;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
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
}

pub struct Cfg {
    pub target: String,
    pub cpu_num: u32,
    pub mem_size: u32,
    pub image: String,
    pub kernel: String,
    // login needed
    pub ssh_key_path: String,
    pub ssh_user: String,
}

/// Handle for qumu instance
#[derive(Debug)]
pub struct Handle {
    done: oneshot::Receiver<tokio::io::Result<ExitStatus>>,
    stdout: mpsc::UnboundedReceiver<BytesMut>,
    stderr: mpsc::UnboundedReceiver<BytesMut>,
}

impl Handle {
    pub fn is_alive(&mut self) -> bool {
        todo!()
    }
}

pub async fn boot(cfg: &Cfg) -> Handle {
    let (mut qemu, qemu_port, _qmp_port) = build_qemu_cmd(cfg);
    let mut handle = qemu
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap_or_else(|e| panic!("Spawn failed:{}", e));

    let stdout = redirect(handle.stdout.take().unwrap());
    let stderr = redirect(handle.stderr.take().unwrap());
    let done = poll_loop(handle);

    let mut handle = Handle {
        done,
        stdout,
        stderr,
    };

    if !boot_success(qemu_port, cfg).await {
        let msg = read_all(&mut handle.stderr).await;
        let err = String::from_utf8(
            msg.into_iter()
                .flat_map(|bs| bs.into_iter())
                .collect::<Vec<u8>>(),
        )
        .unwrap_or(String::from(""));
        panic!("boot failed\n:{}", err);
    }
    clear(&mut handle.stdout).await;
    clear(&mut handle.stderr).await;

    handle
}

async fn boot_success(port: u16, cfg: &Cfg) -> bool {
    const MAX_TRY: u8 = 5;
    // give qemu 15s
    delay_for(Duration::new(15, 0)).await;

    let mut duration = Duration::new(20, 0);
    let mut try_ = 0;
    let test_app = App::new("pwd");

    while try_ < MAX_TRY {
        let ssh_handle = ssh_run(
            &cfg.ssh_key_path,
            &cfg.ssh_user,
            "localhost",
            port,
            Some(test_app.clone()),
        )
        .unwrap_or_else(|e| panic!("Failed to spawn:{}", e));
        if let Ok(r) = timeout(duration, ssh_handle).await {
            if let Ok(status) = r {
                if status.success() {
                    return true;
                }
            }
        }
        duration /= 2;
        try_ += 1;
        delay_for(duration).await;
    }
    false
}

fn build_qemu_cmd(cfg: &Cfg) -> (Command, u16, u16) {
    let default_qemu = QEMUS
        .get(&cfg.target)
        .expect(&format!("Unknown target:{}", &cfg.target))
        .clone();
    let qemu_port = port_check::free_local_port().expect("No free port");
    let qmp_port = port_check::free_local_port().expect("No free port");
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
                    format!("hostfwd=tcp::{}-:22", qemu_port),
                ],
                sp: Some(','),
            },
        ))
        .arg(Arg::new_opt("-hda", OptVal::Normal(cfg.image.clone())))
        .arg(Arg::new_opt("-kernel", OptVal::Normal(cfg.kernel.clone())))
        .arg(Arg::new_opt(
            "-qmp",
            OptVal::Multiple {
                vals: vec![
                    format!("tcp:{}:{}", "localhost", qmp_port),
                    "server".to_string(),
                    "nowait".to_string(),
                ],
                sp: Some(','),
            },
        ))
        .into_cmd();
    (dbg!(qemu), qemu_port, qmp_port)
}

fn redirect<T: AsyncRead + Send + Sync + 'static>(mut src: T) -> mpsc::UnboundedReceiver<BytesMut> {
    let (tx, rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        loop {
            let mut buf = BytesMut::with_capacity(1024);
            if let Ok(_) = src.read_buf(&mut buf).await {
                buf.truncate(buf.len());
                if let Err(_) = tx.send(buf) {
                    break;
                }
            } else {
                break;
            }
        }
    });
    rx
}

fn poll_loop(f: Child) -> oneshot::Receiver<tokio::io::Result<ExitStatus>> {
    let (tx, rx) = oneshot::channel();
    tokio::spawn(async move {
        let status = f.await;
        tx.send(status).unwrap();
    });
    rx
}

async fn clear<T>(rx: &mut mpsc::UnboundedReceiver<T>) {
    while let Ok(v) = rx.try_recv() {
        drop(v);
    }
}

async fn read_all<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Vec<T> {
    let mut buf = Vec::new();
    while let Ok(v) = rx.try_recv() {
        buf.push(v);
    }
    buf
}
