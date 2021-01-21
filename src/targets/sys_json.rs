pub const AKAROS_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/akaros", "/amd64.json"));
pub const FREEBSD_386: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/freebsd", "/386.json"));
pub const FREEBSD_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/freebsd", "/amd64.json"));
pub const FUCHISA_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/fuchsia", "/amd64.json"));
pub const FUCHISA_ARM64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/fuchsia", "/arm64.json"));
pub const NETBSD_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/netbsd", "/amd64.json"));
pub const OPENBSD_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/openbsd", "/amd64.json"));
pub const TRUSTY_ARM: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/trusty", "/arm.json"));
pub const WINDOWS_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/windows", "/amd64.json"));
pub const LINUX_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/amd64.json"));
pub const LINUX_386: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/386.json"));
pub const LINUX_ARM: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/arm.json"));
pub const LINUX_ARM64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/arm64.json"));
pub const LINUX_MIPS64LE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/mips64le.json"));
pub const LINUX_PPC64LE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/ppc64le.json"));
pub const LINUX_RISCV64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/riscv64.json"));
pub const LINUX_S390X: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/s390x.json"));

pub const TARGETS: [(&str, &str); 17] = [
    ("linux/amd64", LINUX_AMD64),
    ("linux/386", LINUX_386),
    ("linux/arm64", LINUX_ARM64),
    ("linux/arm", LINUX_ARM),
    ("linux/mips64le", LINUX_MIPS64LE),
    ("linux/ppc64le", LINUX_PPC64LE),
    ("linux/riscv64", LINUX_RISCV64),
    ("linux/s390x", LINUX_S390X),
    ("akaros/amd64", AKAROS_AMD64),
    ("freebsd/386", FREEBSD_386),
    ("freebsd/amd64", FREEBSD_AMD64),
    ("fuchisa/amd64", FUCHISA_AMD64),
    ("fuchisa/arm64", FUCHISA_ARM64),
    ("netbsd/amd64", NETBSD_AMD64),
    ("openbsd/amd64", OPENBSD_AMD64),
    ("trusty/arm", TRUSTY_ARM),
    ("windows/amd64", WINDOWS_AMD64),
];

pub fn load<T: AsRef<str>>(target: T) -> Option<&'static str> {
    let target = target.as_ref();
    TARGETS
        .iter()
        .copied()
        .find(|t| t.0 == target)
        .map(|(_, desc)| desc)
}

pub fn supported() -> Vec<&'static str> {
    TARGETS.iter().copied().map(|(t, _)| t).collect()
}
