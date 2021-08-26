//! Loading existing syzlang descriptions.
//!
//! Format chain:
//!     Syzlang -> Syzkaller AST -> Json -> Json Object -> Healer AST -> Target

use healer_core::{
    syscall::Syscall,
    target::{Target, TargetBuilder},
    ty::Type,
};
use std::{
    fmt::Display,
    str::FromStr,
    sync::{Mutex, Once},
};

mod convert;

/// akaros/amd64
const AKAROS_AMD64: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/akaros", "/amd64.json"));
/// freeBSD/386
const FREEBSD_386: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/freebsd", "/386.json"));
/// freeBSD/amd64
const FREEBSD_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/freebsd", "/amd64.json"));
/// fuchisa/amd64
const FUCHISA_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/fuchsia", "/amd64.json"));
/// fuchisa/arm64
const FUCHISA_ARM64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/fuchsia", "/arm64.json"));
/// netbsd/amd64
const NETBSD_AMD64: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/netbsd", "/amd64.json"));
/// openbsd/amd64
const OPENBSD_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/openbsd", "/amd64.json"));
/// trusty/arm
const TRUSTY_ARM: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/trusty", "/arm.json"));
/// windows/amd64
const WINDOWS_AMD64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/windows", "/amd64.json"));
/// linux/amd64
const LINUX_AMD64: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/amd64.json"));
/// linux/386
const LINUX_386: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/386.json"));
/// linux/arm
const LINUX_ARM: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/arm.json"));
/// linux/arm64
const LINUX_ARM64: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/arm64.json"));
/// linux/mips64le
const LINUX_MIPS64LE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/mips64le.json"));
/// linux/ppc64le
const LINUX_PPC64LE: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/ppc64le.json"));
/// linux/riscv64
const LINUX_RISCV64: &str =
    include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/riscv64.json"));
/// linux/s396x
const LINUX_S390X: &str = include_str!(concat!(env!("OUT_DIR"), "/sys", "/linux", "/s390x.json"));

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

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SysTarget {
    LinuxAmd64 = 0,
    Linux386,
    LinuxArm64,
    LinuxArm,
    LinuxMpis64le,
    LinuxPpc64le,
    LinuxRiscv64,
    LinuxS390x,
    AkarosAmd64,
    FreeBSD386,
    FreeBSDamd64,
    FuchisaAmd64,
    FuchisaArm64,
    NetBSDAmd64,
    OpenBSDAmd64,
    TrustyArm,
    WindowsAmd64,
}

impl FromStr for SysTarget {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let t = match &s.to_ascii_lowercase()[..] {
            "linux/amd64" => Self::LinuxAmd64,
            "linux/386" => Self::Linux386,
            "linux/arm64" => Self::LinuxArm64,
            "linux/arm" => Self::LinuxArm,
            "linux/mips64le" => Self::LinuxMpis64le,
            "linux/ppc64le" => Self::LinuxPpc64le,
            "linux/riscv64" => Self::LinuxRiscv64,
            "linux/s390x" => Self::LinuxS390x,
            "akaros/amd64" => Self::AkarosAmd64,
            "freebsd/386" => Self::FreeBSD386,
            "freebsd/amd64" => Self::FreeBSDamd64,
            "fuchisa/amd64" => Self::FuchisaAmd64,
            "fuchisa/arm64" => Self::FuchisaArm64,
            "netbsd/amd64" => Self::NetBSDAmd64,
            "openbsd/amd64" => Self::OpenBSDAmd64,
            "trusty/arm" => Self::TrustyArm,
            "windows/amd64" => Self::WindowsAmd64,
            _ => return Err("unsupported target".to_string()),
        };
        Ok(t)
    }
}

#[derive(Debug, Clone)]
pub enum LoadError {
    TargetNotSupported,
    Parse(String),
}

impl Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::TargetNotSupported => write!(f, "target not supported"),
            LoadError::Parse(e) => write!(f, "failed to parse: {}", e),
        }
    }
}

pub fn load_target<T: AsRef<str>>(target: T) -> Result<Target, LoadError> {
    let sys = target
        .as_ref()
        .parse::<SysTarget>()
        .map_err(|_| LoadError::TargetNotSupported)?;
    load_sys_target(sys)
}

pub fn load_sys_target(sys: SysTarget) -> Result<Target, LoadError> {
    static ONCE: Once = Once::new();
    static mut TARGETS_CACHE: Option<Mutex<Vec<Option<Target>>>> = None;
    ONCE.call_once(|| {
        let targets = vec![None; 17];
        unsafe { TARGETS_CACHE = Some(Mutex::new(targets)) };
    });

    let idx = sys as usize;
    let targets_cache = unsafe { TARGETS_CACHE.as_ref().unwrap() };
    let mut targets = targets_cache.lock().unwrap();
    if let Some(target) = &targets[idx] {
        return Ok(target.clone());
    }

    // cache missing, do load
    let description_json = load_description_json(sys)?;
    let (syscalls, tys, res_kinds) = convert::description_json_to_ast(&description_json)?;
    let target = build_target(&description_json, syscalls, tys, res_kinds)?;
    // save to cache
    targets[idx] = Some(target.clone());
    Ok(target)
}

/// Check if current syz-executor build uses shm on `target`
pub fn target_exec_use_shm(target: SysTarget) -> bool {
    let sys_js = load_description_json(target).unwrap();
    let target_json = get(&sys_js, "Target").unwrap();
    target_json["ExecutorUsesShmem"].as_bool().unwrap()
}

/// Check if current syz-executor build uses forksrv on `target`
pub fn target_exec_use_forksrv(target: SysTarget) -> bool {
    let sys_js = load_description_json(target).unwrap();
    let target_json = get(&sys_js, "Target").unwrap();
    target_json["ExecutorUsesForkServer"].as_bool().unwrap()
}

fn build_target(
    descrption_json: &JsonValue,
    syscalls: Vec<Syscall>,
    tys: Vec<Type>,
    res_kinds: Vec<String>,
) -> Result<Target, LoadError> {
    let mut builder = TargetBuilder::new();
    let target_json = get(descrption_json, "Target")?;
    let mut ptrs = vec![0x0000000000000000, 0xffffffffffffffff, 0x9999999999999999];
    let os = get(target_json, "OS")?.as_str().unwrap();
    let arch = get(target_json, "Arch")?.as_str().unwrap();
    if os == "linux" {
        if arch == "amd64" {
            ptrs.push(0xffffffff81000000);
            ptrs.push(0xffffffffff600000);
        } else if arch == "riscv64" {
            ptrs.push(0xffffffe000000000);
            ptrs.push(0xffffff0000000000);
        }
    }
    builder
        .os(os)
        .arch(arch)
        .revision(get(descrption_json, "Revision")?.as_str().unwrap())
        .ptr_sz(get(target_json, "PtrSize")?.as_u64().unwrap())
        .page_sz(get(target_json, "PageSize")?.as_u64().unwrap())
        .page_num(get(target_json, "NumPages")?.as_u64().unwrap())
        .le_endian(get(target_json, "LittleEndian")?.as_bool().unwrap())
        .special_ptrs(ptrs)
        .data_offset(get(descrption_json, "DataOffset")?.as_u64().unwrap())
        .syscalls(syscalls)
        .tys(tys)
        .res_kinds(res_kinds);
    Ok(builder.build())
}

type JsonValue = simd_json::OwnedValue;
use simd_json::prelude::*;

#[inline]
fn get<'a>(val: &'a JsonValue, key: &str) -> Result<&'a JsonValue, LoadError> {
    val.get(key)
        .ok_or_else(|| LoadError::Parse(format!("missing '{}', json:\n{:#}", key, val)))
}

pub fn load_description_json(sys: SysTarget) -> Result<JsonValue, LoadError> {
    static ONCE: Once = Once::new();
    static mut JSONS_CACHE: Option<Mutex<Vec<Option<simd_json::OwnedValue>>>> = None;
    ONCE.call_once(|| {
        let jsons = vec![None; 17];
        unsafe { JSONS_CACHE = Some(Mutex::new(jsons)) };
    });
    let idx = sys as usize;
    let jsons_cache = unsafe { JSONS_CACHE.as_ref().unwrap() };
    let mut jsons = jsons_cache.lock().unwrap();
    if let Some(js) = &jsons[idx] {
        return Ok(js.clone());
    }
    let mut d = TARGETS[idx].1.as_bytes().to_vec();
    let val: simd_json::OwnedValue =
        simd_json::to_owned_value(&mut d).map_err(|e| LoadError::Parse(format!("{}", e)))?;
    jsons[idx] = Some(val.clone());
    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::{load_sys_target, load_target, SysTarget, TARGETS};
    use crate::HashMap;
    use healer_core::corpus::CorpusWrapper;
    use healer_core::gen::{set_prog_len_range, FAVORED_MAX_PROG_LEN, FAVORED_MIN_PROG_LEN};
    use healer_core::parse::parse_prog;
    use healer_core::target::Target;
    use healer_core::{gen, mutation::mutate, relation::Relation};
    use rand::prelude::*;
    use rand::{prelude::SmallRng, SeedableRng};

    #[test]
    fn sys_target_name_parse() {
        for target in &TARGETS {
            target.0.parse::<SysTarget>().unwrap();
        }
    }

    /// Load all targets, make sure the conversion is correct.
    #[test]
    fn load_all_targets() {
        for target in &TARGETS {
            load_target(target.0).unwrap();
        }
    }

    /// Generate 1000 sys prog
    #[test]
    fn sys_prog_gen() {
        let mut rng = SmallRng::from_entropy();
        let target = load_target("linux/amd64").unwrap();
        let relation = Relation::new(&target);
        for _ in 0..4096 {
            gen::gen_prog(&target, &relation, &mut rng);
        }
    }

    fn dummy_corpus(target: &Target, relation: &Relation, rng: &mut SmallRng) -> CorpusWrapper {
        let corpus = CorpusWrapper::new();
        let n = rng.gen_range(8..=32);
        set_prog_len_range(3..8); // progs in corpus are always shorter
        for _ in 0..n {
            let prio = rng.gen_range(64..=1024);
            corpus.add_prog(gen::gen_prog(target, relation, rng), prio);
        }
        set_prog_len_range(FAVORED_MIN_PROG_LEN..FAVORED_MAX_PROG_LEN); // restore
        corpus
    }

    /// Generate 1000 sys prog
    #[test]
    fn sys_prog_mutate() {
        let mut rng = SmallRng::from_entropy();
        let target = load_target("linux/amd64").unwrap();
        let relation = Relation::new(&target);
        let corpus = dummy_corpus(&target, &relation, &mut rng);
        for _ in 0..1024 {
            let mut p = corpus.select_one(&mut rng).unwrap();
            for _ in 0..32 {
                mutate(&target, &relation, &corpus, &mut rng, &mut p);
            }
        }
    }

    #[test]
    fn sys_prog_parse() {
        let mut rng = SmallRng::from_entropy();
        let target = load_target("linux/amd64").unwrap();
        let relation = Relation::new(&target);
        for _ in 0..1024 {
            let p = gen::gen_prog(&target, &relation, &mut rng);
            let p_str = p.display(&target).to_string();
            if let Err(e) = parse_prog(&target, &p_str) {
                println!("{}", p_str);
                println!("{}", e);
                panic!("{}", e)
            }
        }
    }

    // #[test]
    // fn sys_prog_serialize_parse() {
    //     let mut rng = SmallRng::from_entropy();
    //     let target = load_target("linux/amd64").unwrap();
    //     let relation = Relation::new(&target);
    //     for _ in 0..4096 {
    //         let mut p = gen::gen_prog(&target, &relation, &mut rng);
    //         fixup(&target, p.calls_mut());
    //         let p_str = p.display(&target).to_string();
    //         let parsed_p = parse_prog(&target, &p_str).unwrap();
    //         let p_str_2 = parsed_p.display(&target).to_string();
    //         assert_eq!(p_str, p_str_2);
    //     }
    // }

    #[test]
    fn static_relation_basic_attr() {
        let target = load_sys_target(SysTarget::LinuxAmd64).unwrap();
        let relation = Relation::new(&target);
        for (&sa, sx) in relation.influences() {
            assert!(sx.is_empty() || !target.syscall_output_res(sa).is_empty());
            for &sb in sx {
                assert!(!target.syscall_input_res(sb).is_empty());
                assert!(target.syscall_input_res(sb).iter().any(|ir| {
                    target
                        .syscall_output_res(sa)
                        .iter()
                        .any(|or| target.res_sub_tys(ir).contains(or))
                }))
            }
        }
    }

    #[test]
    fn syscall_input_output_res() {
        let target = load_sys_target(SysTarget::LinuxAmd64).unwrap();
        let syscall_input_output_res = vec![
            ("open", vec![], vec!["fd"]),    // direct output
            ("read", vec!["fd"], vec![""]),  // direct input
            ("dup", vec!["fd"], vec!["fd"]), // direct input&output
            (
                "ioctl$MEDIA_IOC_REQUEST_ALLOC",
                vec!["fd_media"],
                vec!["fd_request"],
            ), // output by ptr
            ("bpf$BPF_MAP_FREEZE", vec!["fd_bpf_map"], vec![""]), // input by ptr
            (
                "bpf$MAP_CREATE",
                vec!["fd_bpf_map", "fd_btf", "ifindex"],
                vec!["fd_bpf_map"],
            ), // input by ptr struct
            (
                "ioctl$KVM_CREATE_DEVICE",
                vec!["fd_kvmvm"],
                vec!["fd_kvmdev"],
            ), // output by ptr struct
        ];
        let mut ids = HashMap::default();
        for (name, _, _) in &syscall_input_output_res {
            ids.insert(
                *name,
                target
                    .all_syscalls()
                    .iter()
                    .find(|s| s.name() == *name)
                    .unwrap(),
            );
        }
        for (syscall, expected_ir, expected_or) in syscall_input_output_res {
            let ir = target.syscall_input_res(ids[syscall].id());
            let or = target.syscall_output_res(ids[syscall].id());

            assert!(
                ir.iter().zip(expected_ir.iter()).all(|(a, b)| &a[..] == *b),
                "ir: {:?}, expected: {:?}",
                ir,
                expected_ir
            );
            assert!(
                or.iter().zip(expected_or.iter()).all(|(a, b)| &a[..] == *b),
                "or: {:?}, expected: {:?}",
                or,
                expected_or
            );
        }
    }
}
