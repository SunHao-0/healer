use crate::ssh::ssh_run;
use crate::utils::cli::{App, Arg, OptVal};
use crate::utils::process;
use crate::utils::process::Handle;
use crate::Config;
use std::collections::HashMap;
use tokio::sync::mpsc;
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

#[derive(Debug, Deserialize)]
pub struct Qemu {
    pub target: String,
    pub cpu_num: u32,
    pub mem_size: u32,
    pub image: String,
    pub kernel: String,

    pub wait_boot_time: Option<u8>,
}

pub async fn boot(cfg: &Config) -> (Handle, u16) {
    let (qemu, port) = build_qemu_cli(&cfg.qemu);
    let mut handle = process::spawn(qemu, Some(Duration::new(120, 0)));

    if !is_boot_success(&mut handle, port, cfg).await {
        let msg = read_all(&mut handle.stderr);
        let err = String::from_utf8(
            msg.into_iter()
                .flat_map(|bs| bs.into_iter())
                .collect::<Vec<u8>>(),
        )
        .unwrap_or_default();
        panic!("boot failed\n:{}", err);
    }
    clear(&mut handle.stdout);
    clear(&mut handle.stderr);

    (handle, port)
}

async fn is_boot_success(handle: &mut Handle, port: u16, cfg: &Config) -> bool {
    const MAX_TRY: u8 = 5;

    let wait = cfg.qemu.wait_boot_time.unwrap_or(5);
    delay_for(Duration::new(wait as u64, 0)).await;

    let mut retry_duration = Duration::new(2, 0);
    let ssh_timeout = Duration::new(10, 0);
    let mut try_ = 0;
    let test_app = App::new("pwd");

    while try_ < MAX_TRY {
        if handle.check_if_exit().is_some() {
            return false;
        }

        let ssh_handle = ssh_run(
            &cfg.ssh.key_path,
            &cfg.ssh.user,
            "localhost",
            port,
            test_app.clone(),
        );

        if let Ok(r) = timeout(ssh_timeout, ssh_handle).await {
            if let Ok(status) = r {
                if status.success() {
                    return true;
                }
            }
        }

        delay_for(retry_duration).await;
        retry_duration /= 2;
        try_ += 1;
        println!("Retrying({})", try_);
    }
    false
}

fn build_qemu_cli(cfg: &Qemu) -> (App, u16) {
    let default_qemu = QEMUS
        .get(&cfg.target)
        .unwrap_or_else(|| panic!("Unknown target:{}", &cfg.target))
        .clone();
    let port = port_check::free_local_port().unwrap();

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
                    format!("hostfwd=tcp::{}-:22", port),
                    String::from("hostfwd=tcp::7070-:7070"),
                ],
                sp: Some(','),
            },
        ))
        .arg(Arg::new_opt("-hda", OptVal::Normal(cfg.image.clone())))
        .arg(Arg::new_opt("-kernel", OptVal::Normal(cfg.kernel.clone())));
    (qemu, port)
}

fn clear<T>(rx: &mut mpsc::UnboundedReceiver<T>) {
    while let Ok(v) = rx.try_recv() {
        drop(v);
    }
}

fn read_all<T>(rx: &mut mpsc::UnboundedReceiver<T>) -> Vec<T> {
    let mut buf = Vec::new();
    while let Ok(v) = rx.try_recv() {
        buf.push(v);
    }
    buf
}
