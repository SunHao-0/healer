use std::{
    env,
    fs::{copy, create_dir, read_dir, remove_file, File},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{exit, Command},
};

fn main() {
    let sys_dir = if env::var("SKIP_SYZ_BUILD").is_err() {
        check_env();
        let syz_dir = download();
        build_syz(syz_dir)
    } else if let Ok(syz_dir) = env::var("SYZ_SYS_DIR") {
        PathBuf::from(syz_dir)
    } else {
        let target = env::var("OUT_DIR").unwrap();
        PathBuf::from(&format!("{}/syzkaller/sys/json", target))
    };
    copy_sys(sys_dir)
}

fn check_env() {
    if cfg!(target_os = "windows") {
        eprintln!("Sorry, Healer does not support the Windows platform yet");
        exit(1);
    }

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

fn download() -> PathBuf {
    const DEFAULT_SYZ_REVISION: &str = "6c236867ce33c0c16b102e02a08226d7eb9b2046";
    let syz_revision = if let Ok(revision) = env::var("SYZ_REVISION") {
        revision
    } else {
        DEFAULT_SYZ_REVISION.to_string()
    };

    let repo_url = format!(
        "https://github.com/google/syzkaller/archive/{}.zip",
        syz_revision
    );
    let target = env::var("OUT_DIR").unwrap();
    let syz_zip = PathBuf::from(&format!("{}/syzkaller.zip", target));
    if syz_zip.exists() && !check_download(&syz_zip) {
        remove_file(&syz_zip).unwrap_or_else(|e| {
            eprintln!(
                "failed to removed broken file({}): {}",
                syz_zip.display(),
                e
            );
            exit(1);
        })
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
        if !check_download(&syz_zip) {
            eprintln!("downloaded file {} was broken", syz_zip.display());
            exit(1)
        }
        println!("cargo:rerun-if-changed={}/syzkaller.zip", target);
    }

    let syz_dir = format!("{}/syzkaller-{}", target, syz_revision);
    let syz_dir = PathBuf::from(syz_dir);
    if syz_dir.exists() {
        return syz_dir;
    }

    println!("unziping ...");
    let unzip = Command::new("unzip")
        .current_dir(&target)
        .arg("syzkaller.zip")
        .output()
        .unwrap_or_else(|e| {
            eprintln!("failed to spawn unzip: {}", e);
            exit(1)
        });

    if !unzip.status.success() {
        let stderr = String::from_utf8(unzip.stderr).unwrap_or_default();
        eprintln!("failed to unzip {}: error: {}", syz_zip.display(), stderr);
        exit(1);
    }
    println!("cargo:rerun-if-changed={}", syz_dir.display());
    syz_dir
}

fn check_download<P: AsRef<Path>>(syz_zip: P) -> bool {
    const DEFAULT_CKSUM: &str = "20cc5b9e79b841edba702907b0d1f2646353abc1a3a9bc788640ca49dab42132fde62092480aba49ab236029f3d930e3";
    let expected_csum = if let Ok(csum) = env::var("SYZ_CHECKSUM") {
        csum
    } else {
        DEFAULT_CKSUM.to_string()
    };

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

fn build_syz(syz_dir: PathBuf) -> PathBuf {
    if !syz_dir.join("bin").exists() {
        let patch_dir = PathBuf::from("./patches");
        let headers = vec!["ivshm_setup.h", "features.h"];
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
                        exit(1);
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
                exit(1);
            }
        }
        println!("cargo:rerun-if-changed={}", syz_dir.join("bin").display());
    }

    copy_patched_syz_bin(&syz_dir);
    let sys_dir = syz_dir.join("sys").join("json");
    if !sys_dir.exists() {
        panic!("build finished, no json representation generated");
    }
    println!("cargo:rerun-if-changed={}", sys_dir.display());
    sys_dir
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
