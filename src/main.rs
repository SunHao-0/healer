use healer::{exec::syz::SyzExecConfig, utils::set_debug, vm::qemu::QemuConfig};
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
    /// Enable relation learning.
    #[structopt(long)]
    enable_relation_detect: bool,
    /// Debug mode.
    #[structopt(long)]
    debug: bool,
}

pub fn main() {
    let settings = Settings::from_args();
    let log_conf = ConfigBuilder::new().set_time_format_str("%F %T").build();

    let mut level = LevelFilter::Info;
    if settings.debug {
        set_debug(true);
        level = LevelFilter::Debug;
    }

    TermLogger::init(level, log_conf, TerminalMode::Stdout, ColorChoice::Auto).unwrap();

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
        enable_relation_detect: settings.enable_relation_detect,
        qemu_conf: QemuConfig {
            target: settings.target.clone(),
            kernel_img: settings.kernel_img,
            disk_img: settings.img,
            ssh_key: settings.ssh_key,
            ssh_user: settings.ssh_user,
            qemu_smp: settings.qemu_smp as u32,
            qemu_mem: settings.qemu_mem,
            shmids: Vec::new(),
        },
        exec_conf: SyzExecConfig {
            syz_bin: settings
                .syz_bin_dir
                .join(settings.target.replace("/", "_"))
                .join("syz-executor"),
            force_setup: false,
        },
    };

    healer::start(conf)
}
