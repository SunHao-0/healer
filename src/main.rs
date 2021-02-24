use healer::exec::{ExecConf, QemuConf, SshConf};
use simplelog::*;
use std::path::PathBuf;
use structopt::StructOpt;

#[derive(Debug, StructOpt)]
#[structopt(name = "healer", about = "kernel fuzzer inspired by Syzkaller.")]
struct Settings {
    /// Fuzzing target in Os/Arch format, e.g. linux/amd64, linux/arm64.
    #[structopt(short = "t", long)]
    target: String,
    /// Output directory, contains queues, crashes, relations.
    #[structopt(short = "o", long, default_value = "./output")]
    out_dir: PathBuf,
    /// Directory of object file of target kernel, e.g. vmlinux for linux kernel.
    #[structopt(short = "O", long)]
    kernel_obj_dir: Option<PathBuf>,
    /// Srouce file directory of target kernel.
    #[structopt(short = "S", long)]
    kernel_src_dir: Option<PathBuf>,
    /// Syzkaller binary file directory, syz-executor, syz-symbolize, syz-repro should be provided.
    #[structopt(short = "d", long, default_value = "./bin")]
    syz_bin_dir: PathBuf,
    /// Specify the input relations, [default: 'out_dir/relations].
    #[structopt(short = "r", long)]
    relations: Option<PathBuf>,
    /// Number of parallel instances.
    #[structopt(short = "j", long, default_value = "2")]
    jobs: u64,
    /// Path to disk image.
    #[structopt(short = "i", long)]
    img: PathBuf,
    /// Path to kernel image, e.g. bzImage for linux kernel.
    #[structopt(short = "k", long)]
    kernel_img: Option<PathBuf>,
    /// Number of cpu cores for each qemu.
    #[structopt(long, default_value = "2")]
    qemu_smp: u8,
    /// Size of memory for each qemu in megabyte.
    #[structopt(long, default_value = "2048")]
    qemu_mem: u32,
    /// Path to ssh key used for login to test machine.
    #[structopt(long)]
    ssh_key: PathBuf,
    /// User name for login to test machine.
    #[structopt(long, default_value = "root")]
    ssh_user: String,
    /// Skip crash reproducing.
    #[structopt(long)]
    skip_repro: bool,
    /// File path of disabled calls list.
    #[structopt(long)]
    disabled_calls: Option<PathBuf>,
    /// White list of crash title.
    #[structopt(long)]
    white_list: Option<PathBuf>,
}

pub fn main() {
    let settings = Settings::from_args();
    let log_conf = ConfigBuilder::new().set_time_format_str("%F %T").build();
    TermLogger::init(LevelFilter::Info, log_conf, TerminalMode::Stdout).unwrap();

    let conf = healer::Config {
        target: settings.target.clone(),
        kernel_obj_dir: settings.kernel_obj_dir,
        kernel_src_dir: settings.kernel_src_dir,
        out_dir: settings.out_dir,
        syz_bin_dir: settings.syz_bin_dir.clone(),
        disabled_calls: settings.disabled_calls,
        white_list: settings.white_list,
        jobs: settings.jobs,
        relations: settings.relations,
        skip_repro: settings.skip_repro,
        qemu_conf: QemuConf {
            target: settings.target.clone(),
            img_path: settings.img.into_boxed_path(),
            kernel_path: settings.kernel_img.map(|img| img.into_boxed_path()),
            smp: settings.qemu_smp,
            mem: settings.qemu_mem,
            ..Default::default()
        },
        exec_conf: ExecConf {
            executor: settings
                .syz_bin_dir
                .join(settings.target.replace("/", "_"))
                .join("syz-executor")
                .into_boxed_path(),
        },
        ssh_conf: SshConf {
            ssh_key: settings.ssh_key.into_boxed_path(),
            ssh_user: Some(settings.ssh_user),
        },
    };

    healer::start(conf)
}
