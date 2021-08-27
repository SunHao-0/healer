use std::path::PathBuf;

#[derive(Debug)]
pub struct Settings {
    /// Target os to fuzz.
    pub os: String,
    /// Parallel fuzzing jobs.
    pub job: u64,
    /// Directory to input prog.
    pub input: Option<PathBuf>,
    /// Workdir to write kinds of output data.
    pub workdir: Option<PathBuf>,
    /// Path to kernel image.
    pub kernel_img: Option<PathBuf>,
    /// Path to disk image to boot, default is "stretch.img".
    pub disk_img: PathBuf,
    pub kernel_obj_dir: Option<PathBuf>,
    /// Srouce file directory of target kernel.
    pub kernel_src_dir: Option<PathBuf>,
    /// Directory to syzkaller dir.
    pub syz_dir: PathBuf,
    /// Relations file.
    pub relations: Option<PathBuf>,
    /// Path to ssh secret key to login to os under test.
    pub ssh_key: PathBuf,
    /// Username to login os under test.
    pub ssh_user: String,
    /// Smp, default is 2.
    pub qemu_smp: u32,
    /// Mem size in megabyte.
    pub qemu_mem: u32,
    /// Skip crash reproducing.
    pub skip_repro: bool,
    /// Path to disabled syscalls.
    pub disable_syscalls: Option<PathBuf>,
    /// Path to crash white list.
    pub crash_white_list: Option<PathBuf>,
    #[cfg(debug_assertions)]
    /// Debug mode.
    pub debug: bool,
}
fn main() {}
