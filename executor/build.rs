use std::path::PathBuf;

fn main() {
    let mut files = Vec::new();
    let os = "unix";

    let targets = vec![
        "src/picoc/*.c",
        "src/picoc/*.h",
        "src/picoc/cstdlib/*.c",
        "src/picoc/cstdlib/*.h",
    ];
    for t in targets.into_iter() {
        for f in glob::glob(t).unwrap().filter_map(|f| f.ok()) {
            files.push(f);
        }
    }
    files.push(format!("src/picoc/platform/library_{}.c", os).into());
    files.push(format!("src/picoc/platform/platform_{}.c", os).into());

    assert!(files.contains(&PathBuf::from("src/picoc/platform.c")));

    cc::Build::new()
        .no_default_flags(true)
        .flag("-pedantic")
        .extra_warnings(false)
        .define("UNIX_HOST", None)
        .define("VER", Some("\"2.1\""))
        .define("NO_FP", None)
        .opt_level(1)
        .files(files)
        .file("src/picoc/platform.c") // TODO Emmm, why does glob search failed
        .compile("picoc");

    println!("cargo:rerun-if-changed=src/bind/wrapper.c");
}
