use std::{
    env,
    fs::{copy, create_dir, read_dir, File},
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{exit, Command},
};

fn main() {
    let syz_dir = download();
    let sys_dir = build_syz(syz_dir);
    copy_sys(sys_dir)
}

fn download() -> PathBuf {
    const SYZ_REVISION: &str = "65a7a8540d29e72622fca06522587f7e66539fd3";
    let repo_url = format!(
        "https://github.com/google/syzkaller/archive/{}.zip",
        SYZ_REVISION
    );
    let syz_zip = PathBuf::from("target/syzkaller.zip");
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
        println!("cargo:rerun-if-changed=target/syzkaller.zip");
    }

    let syz_dir = format!("target/syzkaller-{}", SYZ_REVISION);
    let syz_dir = PathBuf::from(syz_dir);
    if syz_dir.exists() {
        return syz_dir;
    }

    println!("unziping ...");
    let unzip = Command::new("unzip")
        .current_dir("target")
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

fn build_syz(syz_dir: PathBuf) -> PathBuf {
    if !syz_dir.join("bin").exists() {
        let patch_file = syz_dir.join("sysgen.patch");
        copy("./sysgen.patch", &patch_file).unwrap_or_else(|e| {
            eprintln!("failed to copy sysgen.patch: {}", e);
            exit(1)
        });

        let patch = Command::new("patch")
            .current_dir(syz_dir.to_str().unwrap())
            .arg("-p1")
            .stdin(File::open("sysgen.patch").unwrap())
            .output()
            .unwrap_or_else(|e| {
                eprintln!("failed to spawn git: {}", e);
                exit(1)
            });
        if !patch.status.success() {
            let stderr = String::from_utf8(patch.stderr).unwrap_or_default();
            eprintln!("failde to patch sysgen: {}", stderr);
            exit(1);
        }

        let make = Command::new("make")
            .current_dir(syz_dir.to_str().unwrap())
            .arg("executor")
            .output()
            .unwrap_or_else(|e| {
                eprintln!("failed to spawn make: {}", e);
                exit(1);
            });
        if !make.status.success() {
            let stderr = String::from_utf8(make.stderr).unwrap_or_default();
            eprintln!("failed to make executor: {}", stderr);
            exit(1);
        }
        println!("cargo:rerun-if-changed={}", syz_dir.join("bin").display());
    }

    let sys_dir = syz_dir.join("sys").join("json");
    if !sys_dir.exists() {
        panic!("build finished, no json representation generated");
    }
    println!("cargo:rerun-if-changed={}", sys_dir.display());
    sys_dir
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
