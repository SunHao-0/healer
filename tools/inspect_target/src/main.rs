use clap::{crate_authors, crate_description, crate_name, crate_version, AppSettings, Clap};
use healer_core::relation::Relation;
use healer_core::target::Target;
use healer_core::ty::TypeKind;
use rand::prelude::*;
use rand::rngs::SmallRng;
use rand::seq::SliceRandom;
use std::cmp::min;
use std::process::exit;
use syz_wrapper::sys;

#[derive(Debug, Clap)]
#[clap(name=crate_name!(), author = crate_authors!(), about = crate_description!(), version = crate_version!())]
#[clap(setting = AppSettings::ColoredHelp)]
struct Settings {
    /// Target to inspect.
    #[clap(long, default_value = "linux/amd64")]
    target: String,
    /// Show resource types.
    #[clap(long, short)]
    res: bool,
    /// Show input/output syscalls of each resource type.
    #[clap(long)]
    res_input_output_syscalls: bool,
    /// Show super type of each resource type.
    #[clap(long)]
    res_super_ty: bool,
    /// Show syscalls.
    #[clap(long, short)]
    syscalls: bool,
    /// Show input/output resource types of each syscall.
    #[clap(long)]
    syscall_input_output_res: bool,
    /// Show relations between syscalls.
    #[clap(long)]
    relation: bool,
    /// Show type information.
    #[clap(long, short = 't')]
    tys: bool,
    /// Only show `filter-ty` type kind.
    #[clap(long)]
    filter_ty: Option<String>,
    /// Show all without sampling.
    #[clap(long, short = 'a')]
    show_all: bool,
    /// Max sampling number.
    #[clap(long, short = 'n', default_value = "64")]
    sample_num: usize,
    /// Seed for random sampling.
    #[clap(long, short = 'r')]
    seed: Option<u64>,
    /// Inspect specific syscall.
    #[clap(long)]
    inspect_syscall: Option<String>,
    /// Inspect specific type.
    #[clap(long)]
    inspect_ty: Option<String>,
    /// Inspect specific resource.
    #[clap(long)]
    inspect_res: Option<String>,
    /// Verbose.
    #[clap(long)]
    verbose: bool,
}

fn main() {
    let settings = Settings::parse();

    let target_name = &settings.target;
    let target = match sys::load_target("linux/amd64") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("failed to load target '{}': {}", target_name, e);
            exit(1);
        }
    };

    let mut rng = if let Some(s) = settings.seed {
        let mut seed: [u8; 32] = [0; 32];
        seed[0..4].copy_from_slice(&s.to_ne_bytes());
        SmallRng::from_seed(seed)
    } else {
        SmallRng::from_entropy()
    };

    println!("description revision: {}", target.revision());
    println!(
        "syscalls: {}, tys: {}, res: {}",
        target.all_syscalls().len(),
        target.tys().len(),
        target.res_tys().len()
    );

    if settings.syscalls || settings.syscall_input_output_res || settings.relation {
        inspect_syscall(&settings, &target, &mut rng);
    }

    if settings.res || settings.res_input_output_syscalls || settings.res_super_ty {
        inspect_res(&settings, &target, &mut rng);
    }

    if settings.tys {
        inspect_ty(&settings, &target, &mut rng);
    }
}

fn inspect_syscall(settings: &Settings, target: &Target, rng: &mut SmallRng) {
    let selected_syscalls = if settings.show_all {
        target.all_syscalls().iter().map(|s| s.id()).collect()
    } else if let Some(syscall_name) = settings.inspect_syscall.as_ref() {
        let syscall = target
            .all_syscalls()
            .iter()
            .filter(|s| s.name() == syscall_name)
            .map(|s| s.id())
            .collect::<Vec<_>>();
        if syscall.is_empty() {
            eprintln!("syscall {} not exist", syscall_name);
            exit(1);
        }
        syscall
    } else {
        let n = min(target.all_syscalls().len(), settings.sample_num);
        target
            .all_syscalls()
            .choose_multiple(rng, n)
            .map(|s| s.id())
            .collect()
    };

    if settings.syscalls {
        println!("======== Syscalls ======== ");
        for sid in &selected_syscalls {
            println!("{}", target.syscall_of(*sid));
        }
    }

    if settings.syscall_input_output_res {
        println!("\n======== Syscalls In/Out Res ======== ");
        for sid in selected_syscalls.iter().copied() {
            let out_res = target.syscall_output_res(sid);
            let in_res = target.syscall_input_res(sid);
            println!(
                "{}: input res: {:?}, output res: {:?}",
                target.syscall_of(sid).name(),
                in_res,
                out_res
            );
        }
    }

    if settings.relation {
        println!("======== Relations ======== ");
        let relation = Relation::new(target);
        for sid in selected_syscalls {
            let influence = relation
                .influence_of(sid)
                .iter()
                .map(|sid| target.syscall_of(*sid).name())
                .collect::<Vec<_>>();
            println!("{}: {:?}", target.syscall_of(sid).name(), influence);
        }
    }
}

fn inspect_res(settings: &Settings, target: &Target, rng: &mut SmallRng) {
    let selected_res = if settings.show_all {
        target.res_kinds().to_vec()
    } else if let Some(res_name) = settings.inspect_res.as_ref() {
        let res_name = res_name.to_string().into_boxed_str();
        let res = target.res_kinds().binary_search(&res_name);
        if res.is_err() {
            eprintln!("resource {} not exist", res_name);
            exit(1);
        }
        vec![res_name]
    } else {
        let n = min(target.res_kinds().len(), settings.sample_num);
        target
            .res_kinds()
            .choose_multiple(rng, n)
            .cloned()
            .collect()
    };

    if settings.res {
        println!("\n======== Res Type ========");
        for tid in &selected_res {
            if let Some(tys) = target.res_ty_of(tid) {
                for tid in tys {
                    let res = target.ty_of(*tid).checked_as_res();
                    println!("{}{:?} = {:?}", res.name(), res.kinds(), res.special_vals());
                }
            }
        }
    }

    if settings.res_input_output_syscalls {
        println!("\n======== Res In/Out Syscall ======== ");
        for res in selected_res.iter() {
            let out_syscalls = target
                .res_output_syscall(res)
                .iter()
                .map(|sid| target.syscall_of(*sid).name().to_string())
                .collect::<Vec<_>>();
            let in_syscalls = target
                .res_input_syscall(res)
                .iter()
                .map(|sid| target.syscall_of(*sid).name().to_string())
                .collect::<Vec<_>>();
            println!(
                "{}:\n\tinput syscalls: {:?}\n\toutput sycalls: {:?}\n",
                res, in_syscalls, out_syscalls
            );
        }
    }

    if settings.res_super_ty {
        println!("\n======== Res Super/Sub Type ========");
        for res in &selected_res {
            let super_tys = target.res_super_tys(res);
            let sub_tys = target.res_sub_tys(res);
            println!("{}: {:?}, {:?}", res, super_tys, sub_tys);
        }
    }
}

fn inspect_ty(settings: &Settings, target: &Target, rng: &mut SmallRng) {
    let selected_tys = if settings.show_all {
        target.tys().iter().map(|ty| ty.id()).collect()
    } else if let Some(ty_name) = settings.inspect_ty.as_ref() {
        let ty = target
            .tys()
            .iter()
            .filter(|ty| ty.name() == ty_name)
            .map(|ty| ty.id())
            .collect::<Vec<_>>();
        if ty.is_empty() {
            eprintln!("resource {} not exist", ty_name);
            exit(1);
        }
        ty
    } else if let Some(filter_ty) = &settings.filter_ty {
        let kind = parse_ty_kind(filter_ty);
        let tys = target
            .tys()
            .iter()
            .filter(|ty| ty.kind() == kind)
            .map(|ty| ty.id())
            .collect::<Vec<_>>();
        if tys.is_empty() {
            eprintln!("ty of kind {} not exist", filter_ty);
            exit(1);
        }
        tys
    } else {
        let n = min(target.tys().len(), settings.sample_num);
        target
            .tys()
            .choose_multiple(rng, n)
            .map(|r| r.id())
            .collect()
    };

    if settings.tys {
        for tid in selected_tys {
            if settings.verbose {
                println!("{:?}", target.ty_of(tid))
            } else {
                println!("{}", target.ty_of(tid))
            }
        }
    }
}

fn parse_ty_kind(kind_str: &str) -> TypeKind {
    use TypeKind::*;
    match &kind_str.to_ascii_lowercase()[..] {
        "res" => Res,
        "const" => Const,
        "int" => Int,
        "flags" => Flags,
        "len" => Len,
        "proc" => Proc,
        "csum" => Csum,
        "vma" => Vma,
        "bufferblob" | "blob" => BufferBlob,
        "bufferstring" | "string" => BufferString,
        "bufferfilename" | "filename" => BufferFilename,
        "array" => Array,
        "ptr" => Ptr,
        "struct" => Struct,
        "union" => Union,
        _ => {
            eprintln!("unknown type kind: {}", kind_str);
            exit(1)
        }
    }
}
