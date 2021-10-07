use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};
use env_logger::{Env, TimestampPrecision};
use healer_fuzzer::{boot, config::Config};
use healer_vm::qemu::QemuConfig;
use std::path::PathBuf;
use syz_wrapper::{report::ReportConfig, repro::ReproConfig};
#[derive(Debug, Clap)]
#[clap(version = crate_version!(), author=crate_authors!(), about=crate_description!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Settings {
    /// Target os to fuzz.
    #[clap(long, short = 'O', default_value = "linux")]
    os: String,
    /// Parallel fuzzing jobs.
    #[clap(long, short = 'j', default_value = "4")]
    job: u64,
    /// Directory to input prog.
    #[clap(long, short = 'i')]
    input: Option<PathBuf>,
    /// Directory to write kinds of output data.
    #[clap(long, short = 'o', default_value = "output")]
    output: PathBuf,
    /// Path to kernel image.
    #[clap(long, short = 'k', default_value = "bzImage")]
    kernel_img: PathBuf,
    /// Path to disk image.
    #[clap(long, short = 'd', default_value = "stretch.img")]
    disk_img: PathBuf,
    /// Directory of target kernel object.
    #[clap(long, short = 'b')]
    kernel_obj_dir: Option<PathBuf>,
    /// Srouce file directory of target kernel.
    #[clap(long, short = 'r')]
    kernel_src_dir: Option<PathBuf>,
    /// Directory to syzkaller dir.
    #[clap(long, short = 'S', default_value = "./")]
    syz_dir: PathBuf,
    /// Relations file.
    #[clap(long, short = 'R')]
    relations: Option<PathBuf>,
    /// Path to ssh secret key to login to os under test.
    #[clap(long, short = 's', default_value = "./stretch.id_rsa")]
    ssh_key: PathBuf,
    /// Username to login os under test.
    #[clap(long, short = 'u', default_value = "root")]
    ssh_user: String,
    /// QEMU smp.
    #[clap(long, short = 'c', default_value = "2")]
    qemu_smp: u32,
    /// QEMU mem size in megabyte.
    #[clap(long, short = 'm', default_value = "4096")]
    qemu_mem: u32,
    /// Path to disabled syscalls.
    #[clap(long)]
    disable_syscalls: Option<PathBuf>,
    /// Path to crash white list.
    #[clap(long)]
    crash_whitelist: Option<PathBuf>,
    /// Number of instance used for repro.
    #[clap(long, default_value = "2")]
    repro_vm_count: u64,
    /// Disable call fault injection.
    #[clap(long)]
    disable_fault_injection: bool,
    /// Whitelist for fault injection.
    #[clap(long)]
    fault_injection_whitelist: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let settings = Settings::parse();

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
