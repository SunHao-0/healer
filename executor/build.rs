use std::env;
use std::fs::create_dir;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    if cfg!(feature = "interprete") {
        bind_picoc()
    } else if cfg!(feature = "jit") {
        let tcc_src = env::current_dir().unwrap().join("tcc-0.9.27");
        let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
        let build_dir = out_dir.join("build");
        println!("TCC-SRC:{}", tcc_src.display());
        println!("OUT_DIR:{}", out_dir.display());
        println!("BUILD_DIR:{}", build_dir.display());
        if let Err(e) = create_dir(&build_dir) {
            if let ErrorKind::AlreadyExists = e.kind() {
                println!("WARN: BUILD_DIR already exists");
            } else {
                panic!(e);
            }
        }

        let conf_status = Command::new(tcc_src.join("configure"))
            .arg(format!("--prefix={}", out_dir.display()))
            .arg("--extra-cflags=-O3 -fPIC -g -Wall")
            .current_dir(&build_dir)
            .status()
            .unwrap();
        if !conf_status.success() {
            panic!("Fail to configure: {:?}", conf_status)
        }

        let make_status = Command::new("make")
            .current_dir(&build_dir)
            .arg(format!(
                "-j{}",
                env::var("NUM_JOBS").unwrap_or_else(|_| String::from("1"))
            ))
            .status()
            .unwrap();
        if !make_status.success() {
            panic!("Fail to make: {:?}", conf_status)
        }

        let install_status = Command::new("make")
            .current_dir(&build_dir)
            .arg("install")
            .status()
            .unwrap();
        if !install_status.success() {
            panic!("Fail to install: {:?}", conf_status)
        }

        println!("cargo:rustc-link-search=native={}/lib", out_dir.display());
        println!("cargo:rustc-link-lib=static=tcc");
        println!("cargo:rerun-if-changed={}", tcc_src.display());
    }
}

fn bind_picoc() {
    bind_gen();
    build_picoc()
}

fn bind_gen() {
    const HEADER: &str = "picoc/ffi.h";
    println!("cargo:rerun-if-changed={}", HEADER);
    let bingings = bindgen::Builder::default()
        .header(HEADER)
        .derive_copy(false)
        .derive_default(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .generate()
        .expect("Unable to generate bindings");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bingings
        .write_to_file(out.join("bind_unix.rs"))
        .unwrap_or_else(|e| panic!("Unable to write bindings: {}", e))
}

fn build_picoc() {
    let mut files = Vec::new();
    let os = "unix";

    let cstdlibs = vec![
        "ctype.c",
        "errno.c",
        "fcntl.c",
        "math.c",
        "stdbool.c",
        "types.c",
        "stdio.c",
        "stdlib.c",
        "string.c",
        "time.c",
        "unistd.c",
        "sys.c",
    ];
    let picoc_srcs = vec![
        "clibrary.c",
        "debug.c",
        "expression.c",
        "heap.c",
        "include.c",
        "interpreter.h",
        "lex.c",
        "parse.c",
        "picoc.h",
        "platform.c",
        "platform.h",
        "table.c",
        "type.c",
        "variable.c",
    ];
    for picoc_file in &picoc_srcs {
        files.push(format!("picoc/{}", picoc_file));
    }
    for cstdlib in &cstdlibs {
        files.push(format!("picoc/cstdlib/{}", cstdlib))
    }

    files.push(format!("picoc/platform/library_{}.c", os));
    files.push(format!("picoc/platform/platform_{}.c", os));

    cc::Build::new()
        .no_default_flags(true)
        .flag("-pedantic")
        .extra_warnings(false)
        .define("UNIX_HOST", None)
        .define("VER", Some("\"2.1\""))
        .define("NO_FP", None)
        .opt_level(3)
        .files(files)
        .file("picoc/platform.c")
        .compile("picoc");
}
