//! Syz-wrapper build script.
//!
//! This build script downloads, patches and builds Syzkaller.
//! The Syzlang description will be dump to json files in `OUT_DIR/sys/json`.
//! All the json format syscall descriptions will be included to source code
//! with `include_str!` macro (see `sys/mod.rs`), so that Healer can load them
//! without further manual efforts.
//! One can also skip this process and provide their own build via some extra env vars.
use std::{
    env,
    fs::{copy, create_dir, read_dir, remove_file, File},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{exit, Command},
};

/// Revision that the patches can be applied stably.
const STABLE_REVISION: &str = "169724fe58e8d7d0b4be6f59ca7c1e0f300399e1";
const STABLE_CSUM: &str = "293a65f4604dce1103ca94746fec6bb175229576271ffdcd319747cc33db2b89f5467acea86a2bf8b38c0fc95adea3c0";

fn main() {
    if env::var("SKIP_SYZ_BUILD").is_err() {
        check_env();
        // TODO We cannot use the latest syz-executor any more, because `a7ce77be27d8e3728b97122a005bc5b23298cfc3` contains breaking change
        // Try to patch the latest revision first
        // const LATEST_REVISION: &str = "master";
        // let syz_dir = download(LATEST_REVISION, None);
        // if let Some(sys_dir) = build_syz(syz_dir) {
        // copy_sys(sys_dir);
        // } else {
        eprintln!("failed to patch and build latest revision, failback...");
        let syz_dir = download(STABLE_REVISION, Some(STABLE_CSUM));
        if let Some(sys_dir) = build_syz(syz_dir) {
            copy_sys(sys_dir);
            return;
        }
        eprintln!(
            "failed to build and patch Syzkaller with stable revision ({})",
            STABLE_REVISION
        );
        exit(1)
        // }
    } else if let Ok(sys_dir) = env::var("SYZ_SYS_DIR") {
        let sys_dir = PathBuf::from(sys_dir);
        copy_sys(sys_dir)
    } else {
        eprintln!("Directory that contains json format Syzlang description should be provided via `SYZ_SYS_DIR` env var, 
        when `SKIP_SYZ_BUILD` env var is set");
        exit(1)
    };
}

/// Check required tool to build Syzkaller
fn check_env() {
    const TOOLS: [(&str, &str); 6] = [
        ("wget", "download syzkaller"),
        ("sha384sum", "check download"),
        ("unzip", "unzip syzkaller.zip"),
        ("patch", "patch patches/*.diff"),
        ("make", "build the syzkaller description and executor"),
        ("go", "build the syzkaller"),
    ];
    let mut missing = false;
    for (tool, reason) in TOOLS.iter().copied() {
        let status = Command::new("which").arg(tool).status().unwrap();
        if !status.success() {
            eprintln!("missing tool {} to {}.", tool, reason);
            missing = true;
        }
    }
    if missing {
        eprintln!("missing tools, please install them first");
        exit(1)
    }
}

fn download(syz_revision: &str, csum: Option<&str>) -> PathBuf {
    let repo_url = format!(
        "https://github.com/google/syzkaller/archive/{}.zip",
        syz_revision
    );
    let target = env::var("OUT_DIR").unwrap();
    let syz_zip = PathBuf::from(&format!("{}/syzkaller-{}.zip", target, syz_revision));
    let syz_dir = format!("{}/syzkaller-{}", target, syz_revision);
    let syz_dir = PathBuf::from(syz_dir);
    let mut need_unzip = true;

    if syz_dir.exists() {
        return syz_dir;
    }

    if syz_zip.exists() {
        let mut need_remove = false;
        if let Some(expected_csum) = csum {
            need_remove = !check_download_csum(&syz_zip, expected_csum);
        } else if try_unzip(&target, &syz_zip) {
            need_unzip = false;
        } else {
            need_remove = true;
        };

        if need_remove {
            remove_file(&syz_zip).unwrap_or_else(|e| {
                eprintln!(
                    "failed to removed broken file({}): {}",
                    syz_zip.display(),
                    e
                );
                exit(1);
            })
        }
    }

    if !syz_zip.exists() {
        println!("downloading syzkaller...");
        let wget = Command::new("wget")
            .arg("-O")
            .arg(syz_zip.to_str().unwrap())
            .arg(&repo_url)
            .output()
            .unwrap_or_else(|e| {
                eprintln!("failed to spawn wget: {}", e);
                exit(1)
            });
        if !wget.status.success() {
            let stderr = String::from_utf8(wget.stderr).unwrap_or_default();
            eprintln!(
                "failed to download syzkaller from: {}, error: {}",
                repo_url, stderr
            );
            exit(1);
        }
        if let Some(csum) = csum {
            if !check_download_csum(&syz_zip, csum) {
                eprintln!("downloaded file {} was broken", syz_zip.display());
                exit(1);
            }
        }
        println!("cargo:rerun-if-changed={}", syz_zip.display());
    }

    if need_unzip && !try_unzip(&target, &syz_zip) {
        eprintln!("failed to unzip the downloaded file: {}", syz_zip.display());
        exit(1);
    }

    assert!(syz_dir.exists());
    println!("cargo:rerun-if-changed={}", syz_dir.display());
    syz_dir
}

fn try_unzip<P: AsRef<Path>>(current_dir: &str, syz_zip: P) -> bool {
    let unzip = Command::new("unzip")
        .current_dir(current_dir)
        .arg(syz_zip.as_ref())
        .output()
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn unzip: {}", e);
            exit(1)
        });
    unzip.status.success()
}

fn check_download_csum<P: AsRef<Path>>(syz_zip: P, expected_csum: &str) -> bool {
    let output = Command::new("sha384sum")
        .arg(syz_zip.as_ref())
        .output()
        .unwrap();
    if !output.status.success() {
        let stderr = String::from_utf8(output.stderr).unwrap_or_default();
        eprintln!("sha384sum failed: {}", stderr);
        exit(1)
    } else {
        let stdout = String::from_utf8(output.stdout).unwrap();
        let cksum = stdout.split(' ').next().unwrap();
        cksum.trim() == expected_csum
    }
}

fn build_syz(syz_dir: PathBuf) -> Option<PathBuf> {
    if !syz_dir.join("bin").exists() {
        let patch_dir = PathBuf::from("./patches");
        let headers = vec!["ivshm_setup.h", "features.h", "unix_sock_setup.h"];
        for header in headers {
            let to = format!("executor/{}", header);
            copy(patch_dir.join(&header), syz_dir.join(&to)).unwrap_or_else(|e| {
                eprintln!("failed to copy {} to {}: {}", header, to, e);
                exit(1)
            });
        }

        for f in read_dir(&patch_dir).unwrap().filter_map(|f| f.ok()) {
            let f = f.path();
            if let Some(ext) = f.extension() {
                if ext.to_str().unwrap() == "diff" {
                    let patch_file = syz_dir.join(f.file_name().unwrap());
                    copy(f, &patch_file).unwrap_or_else(|e| {
                        eprintln!(
                            "failed to copy patch file '{}': {}",
                            patch_file.display(),
                            e
                        );
                        exit(1)
                    });
                    let patch = Command::new("patch")
                        .current_dir(syz_dir.to_str().unwrap())
                        .arg("-p1")
                        .stdin(File::open(&patch_file).unwrap())
                        .output()
                        .unwrap_or_else(|e| {
                            eprintln!("failed to spawn git: {}", e);
                            exit(1)
                        });
                    if !patch.status.success() {
                        let stderr = String::from_utf8(patch.stderr).unwrap_or_default();
                        eprintln!("failde to patch {}: {}", patch_file.display(), stderr);
                        return None;
                    }
                }
            }
        }

        let targets = vec!["executor", "fuzzer", "execprog", "repro", "symbolize"];
        for target in targets {
            let make = Command::new("make")
                .current_dir(syz_dir.to_str().unwrap())
                .arg(target)
                .output()
                .unwrap_or_else(|e| {
                    eprintln!("failed to spawn make: {}", e);
                    exit(1);
                });
            if !make.status.success() {
                let stderr = String::from_utf8(make.stderr).unwrap_or_default();
                eprintln!("failed to make {}: {}", target, stderr);
                return None;
            }
        }
        println!("cargo:rerun-if-changed={}", syz_dir.join("bin").display());
    }

    copy_patched_syz_bin(&syz_dir);
    let sys_dir = syz_dir.join("sys").join("json");
    assert!(sys_dir.exists());
    println!("cargo:rerun-if-changed={}", sys_dir.display());
    Some(sys_dir)
}

fn copy_patched_syz_bin(syz_dir: &Path) {
    use std::os::unix::fs::symlink;

    let bin_dir = syz_dir.join("bin");
    if !bin_dir.exists() {
        eprintln!("executable files not exist: {}", syz_dir.display());
        exit(1);
    }
    // target/[debug/release]/build/syz-wrapper/out/..
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_bin = out_dir
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("syz-bin");
    println!(
        "copy bin from {} to {}...",
        bin_dir.display(),
        out_bin.display()
    );
    if let Err(e) = symlink(&bin_dir, &out_bin) {
        if e.kind() != ErrorKind::AlreadyExists {
            eprintln!(
                "failed to hardlink bin dir from {} to {}: {}",
                bin_dir.display(),
                out_bin.display(),
                e
            );
            exit(1);
        }
    }
}

fn copy_sys(sys_dir: PathBuf) {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let out_sys = out_dir.join("sys");
    if let Err(e) = create_dir(&out_sys) {
        if e.kind() != ErrorKind::AlreadyExists {
            eprintln!("failed to create out sys dir: {}", e);
            exit(1)
        }
    }

    for f in read_dir(sys_dir).unwrap().filter_map(|f| f.ok()) {
        let p = f.path();
        if p.is_dir() {
            let out = out_sys.join(p.file_name().unwrap());
            if let Err(e) = create_dir(&out) {
                if e.kind() == ErrorKind::AlreadyExists {
                    continue;
                } else {
                    eprintln!("failed to create out os sys dir: {}", e);
                    exit(1)
                }
            }
            copy_dir_json(&p, &out)
        }
    }
    println!("cargo:rerun-if-changed={}", out_sys.display());
}

fn copy_dir_json<P: AsRef<Path>>(from: P, to: P) {
    for f in read_dir(from.as_ref()).unwrap().filter_map(|f| f.ok()) {
        let p = f.path();
        if let Some(ext) = p.extension() {
            if ext.to_str().unwrap() == "json" {
                let fname = p.file_name().unwrap();
                let to_fname = to.as_ref().join(fname);
                copy(&p, &to_fname).unwrap_or_else(|e| {
                    eprintln!(
                        "failed to copy from {} to {}: {}",
                        p.display(),
                        to_fname.display(),
                        e
                    );
                    exit(1)
                });
            }
        }
    }
}
