//! Boot up and manage virtual machine

use std::path::PathBuf;

const QEMU_X86_64_LINUX: &str = "qemu-system-x86_64 -no-reboot -display none -serial stdio -snapshot
-enable-kvm 
-cpu host,migratable=off 
-net nic,model=e1000 
-append \"earlyprintk=serial oops=panic nmi_watchdog=panic panic_on_warn=1 panic=1 ftrace_dump_on_oops=orig_cpu rodata=n vsyscall=native
net.ifnames=0 biosdevname=0 root=/dev/sda console=tty50 kvm-intel.nested=1
kvm-intel.unrestricted_guest=1 kvm-intel.vmm_exclusive=1 kvm-intel.fasteoi=1 
kvm-intel.ept=1 kvm-intel.flexpriority=1 kvm-intel.vpid=1 kvm-intel.emulate_invalid_guest_state=1
kvm-intel.eptad=1 kvm-intel.enable_shadow_vmcs=1 kvm-intel.pml=1 kvm-intel.enable_apicv=1\"";

const QEMU_ARM_LINUX: &str = "qemu-system-arm  -no-reboot -display none -serial stdio -snapshot
 -net nic
 -append \"root=/dev/vda console=ttyAMA0\"";

const QEMU_ARM64_LINUX: &str =
    "qemu-system-aarch64 -no-reboot -display none -serial stdio -snapshot
 -machine virt,virtualization=on 
 -cpu cortex-a57
 -net nic
 -append \"root=/dev/vda console=ttyAMA0\"";

pub struct VmHandle;

pub struct Config {
    kernel: PathBuf,
    img_path: PathBuf,
    smp: u8,
}

pub enum BootTarget {
    LinuxQemuAmd64,
    LinuxQemuArm,
    LinuxQemuArm64,
}

pub fn boot(target: BootTarget, cfg: Config) -> Result<String, VmHandle> {
    // Need comfirm "-smp {} -net user,host=10.0.2.10,hostfwd=tcp::{}-:22 -hda {} -kernel {}
    // -device ivshmem-plain, memdev=hostmem
    // -object memory-backend-file,，size={},share,mem-path={},id=hostmem
    todo!()
}
