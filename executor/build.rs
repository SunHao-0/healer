use std::env;
use std::path::PathBuf;

fn main() {
    if cfg!(feature = "interprete") {
        bind_picoc()
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

    files.push(format!("picoc/platform/library_{}.c", os).into());
    files.push(format!("picoc/platform/platform_{}.c", os).into());

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
