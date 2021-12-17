use env_logger::{Env, TimestampPrecision};
use healer_fuzzer::{boot, config::Config};
use healer_vm::qemu::QemuConfig;
use std::path::PathBuf;
use structopt::StructOpt;
use syz_wrapper::{report::ReportConfig, repro::ReproConfig};

#[derive(Debug, StructOpt)]
struct Settings {
    /// Target os to fuzz.
    #[structopt(long, short = "O", default_value = "linux")]
    os: String,
    /// Parallel fuzzing jobs.
    #[structopt(long, short = "j", default_value = "4")]
    job: u64,
    /// Directory to input prog.
    #[structopt(long, short = "i")]
    input: Option<PathBuf>,
    /// Directory to write kinds of output data.
    #[structopt(long, short = "o", default_value = "output")]
    output: PathBuf,
    /// Path to kernel image.
    #[structopt(long, short = "k", default_value = "bzImage")]
    kernel_img: PathBuf,
    /// Path to disk image.
    #[structopt(long, short = "d", default_value = "stretch.img")]
    disk_img: PathBuf,
    /// Directory of target kernel object.
    #[structopt(long, short = "b")]
    kernel_obj_dir: Option<PathBuf>,
    /// Srouce file directory of target kernel.
    #[structopt(long, short = "r")]
    kernel_src_dir: Option<PathBuf>,
    /// Directory to syzkaller dir.
    #[structopt(long, short = "S", default_value = "./")]
    syz_dir: PathBuf,
    /// Relations file.
    #[structopt(long, short = "R")]
    relations: Option<PathBuf>,
    /// Path to ssh secret key to login to os under test.
    #[structopt(long, short = "s", default_value = "./stretch.id_rsa")]
    ssh_key: PathBuf,
    /// Username to login os under test.
    #[structopt(long, short = "u", default_value = "root")]
    ssh_user: String,
    /// QEMU smp.
    #[structopt(long, short = "c", default_value = "2")]
    qemu_smp: u32,
    /// QEMU mem size in megabyte.
    #[structopt(long, short = "m", default_value = "4096")]
    qemu_mem: u32,
    /// Path to disabled syscalls.
    #[structopt(long)]
    disable_syscalls: Option<PathBuf>,
    /// Path to crash white list.
    #[structopt(long)]
    crash_whitelist: Option<PathBuf>,
    /// Number of instance used for repro.
    #[structopt(long, default_value = "2")]
    repro_vm_count: u64,
    /// Disable call fault injection.
    #[structopt(long)]
    disable_fault_injection: bool,
    /// Whitelist for fault injection.
    #[structopt(long)]
    fault_injection_whitelist: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let settings = Settings::from_args();

    let log_env = Env::new()
        .filter_or("HEALER_LOG", "info")
        .default_write_style_or("auto");
    env_logger::Builder::from_env(log_env)
        .format_timestamp(Some(TimestampPrecision::Seconds))
        .init();

    let config = Config {
        os: settings.os,
        relations: settings.relations,
        input: settings.input,
        crash_whitelist: settings.crash_whitelist,
        job: settings.job,
        syz_dir: settings.syz_dir,
        output: settings.output,
        disabled_calls: settings.disable_syscalls,
        disable_fault_injection: settings.disable_fault_injection,
        fault_injection_whitelist_path: settings.fault_injection_whitelist,
        qemu_config: QemuConfig {
            qemu_smp: settings.qemu_smp,
            qemu_mem: settings.qemu_mem,
            ssh_key: settings.ssh_key.to_str().unwrap().to_string(),
            ssh_user: settings.ssh_user,
            kernel_img: Some(settings.kernel_img.to_str().unwrap().to_string()),
            disk_img: settings.disk_img.to_str().unwrap().to_string(),
            ..Default::default()
        },
        repro_config: ReproConfig {
            qemu_count: settings.repro_vm_count,
            ..Default::default()
        },
        report_config: ReportConfig {
            kernel_obj_dir: settings
                .kernel_obj_dir
                .map(|s| s.to_str().unwrap().to_string()),
            kernel_src_dir: settings
                .kernel_src_dir
                .map(|s| s.to_str().unwrap().to_string()),
            ..Default::default()
        },
        ..Default::default()
    };

    boot(config)
}
