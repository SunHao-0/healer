use crate::ssh::ssh_run;
use crate::utils::cli::{App, Arg, OptVal};
use crate::utils::process;
use crate::utils::process::Handle;
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

pub async fn boot(cfg: &Cfg) -> Handle {
    let (qemu, qemu_port, _qmp_port) = build_qemu_cli(cfg);
    let mut handle = process::spawn(qemu, Some(Duration::new(120, 0)));

    if !is_boot_success(qemu_port, cfg).await {
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

    handle
}

async fn is_boot_success(port: u16, cfg: &Cfg) -> bool {
    const MAX_TRY: u8 = 5;
    println!("Waiting qemu 10s ...");
    delay_for(Duration::new(10, 0)).await;

    let mut retry_duration = Duration::new(10, 0);
    let ssh_timeout = Duration::new(10, 0);
    let mut try_ = 0;
    let test_app = App::new("pwd");

    while try_ < MAX_TRY {
        let ssh_handle = ssh_run(
            &cfg.ssh_key_path,
            &cfg.ssh_user,
            "localhost",
            port,
            Some(test_app.clone()),
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

fn build_qemu_cli(cfg: &Cfg) -> (App, u16, u16) {
    let default_qemu = QEMUS
        .get(&cfg.target)
        .unwrap_or_else(|| panic!("Unknown target:{}", &cfg.target))
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
        ));
    (qemu, qemu_port, qmp_port)
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
