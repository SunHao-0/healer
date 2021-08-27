#[cfg(target_arch = "x86_64")]
pub static TARGET_ARCH: &str = "amd64";

#[cfg(target_arch = "x86")]
pub static TARGET_ARCH: &str = "386";

#[cfg(target_arch = "aarch64")]
pub static TARGET_ARCH: &str = "arm64";

#[cfg(target_arch = "arm")]
pub static TARGET_ARCH: &str = "arm";

#[cfg(target_arch = "mips64el")]
pub static TARGET_ARCH: &str = "mips64le";

#[cfg(target_arch = "ppc64")]
pub static TARGET_ARCH: &str = "ppc64le";

#[cfg(target_arch = "riscv64")]
pub static TARGET_ARCH: &str = "riscv64";

#[cfg(target_arch = "s390x")]
pub static TARGET_ARCH: &str = "s390x";
