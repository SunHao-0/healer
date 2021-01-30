use healer::{
    exec::{ExecConf, QemuConf, SshConf},
    fuzz::log,
    Config,
};
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "healer", about = "kernel fuzzer inspired by Syzkaller.")]
struct Settings {
    /// Supported target in Os/Arch format, e.g. linux/amd64, linux/arm64. See target/sys_json.rs.
    #[structopt(short = "t", long)]
    target: String,
    /// Working directory of healer, should at least contain bin/syz-executor, bin/syz-symbolizer.
    #[structopt(short = "w", long, default_value = "./")]
    work_dir: PathBuf,
    /// Object file of target kernel, e.g. vmlinux for linux kernel.
    #[structopt(short = "obj", long)]
    kernel_obj: Option<PathBuf>,
    /// Srouce file of target kernel.
    #[structopt(short = "src", long)]
    kernel_src: Option<PathBuf>,
    /// Number of parallel instances.
    #[structopt(short, long, default_value = "2")]
    jobs: u64,
    /// Path to disk image.
    #[structopt(short, long)]
    img: PathBuf,
    /// Path to kernel image, e.g. bzImage for linux kernel.
    #[structopt(short, long)]
    kernel_img: Option<PathBuf>,
    /// Number of cpu cores for each qemu.
    #[structopt(short = "smp", long, default_value = "2")]
    qemu_smp: u8,
    /// Size of memory for each qemu in megabyte.
    #[structopt(short = "mem", long, default_value = "2048")]
    qemu_mem: u8,
    /// Path to ssh key used for logging to test machine.
    #[structopt(short = "key", long)]
    ssh_key: PathBuf,
    /// User name for logging to test machine.
    #[structopt(short = "user", long, default_value = "root")]
    ssh_user: String,
}

pub fn main() {
    let settings = Settings::from_args();
    log::init();
    let conf = Config {
        target: settings.target.clone(),
        kernel_obj: settings.kernel_obj,
        kernel_src: settings.kernel_src,
        jobs: settings.jobs,
        qemu_conf: QemuConf {
            target: settings.target,
            img_path: settings.img.into_boxed_path(),
            kernel_path: settings.kernel_img.map(|img| img.into_boxed_path()),
            smp: Some(settings.qemu_smp),
            mem: Some(settings.qemu_mem),
            ..Default::default()
        },
        exec_conf: ExecConf {
            executor: settings
                .work_dir
                .join("bin")
                .join("syz-executor")
                .into_boxed_path(),
        },
        ssh_conf: SshConf {
            ssh_key: settings.ssh_key.into_boxed_path(),
            ssh_user: Some(settings.ssh_user),
        },
        work_dir: settings.work_dir,
    };

    healer::start(conf)
}
