fn main() {
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
        "sys.c"
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
        files.push(format!("src/picoc/{}", picoc_file));
    }
    for cstdlib in &cstdlibs {
        files.push(format!("src/picoc/cstdlib/{}", cstdlib))
    }

    files.push(format!("src/picoc/platform/library_{}.c", os).into());
    files.push(format!("src/picoc/platform/platform_{}.c", os).into());

    cc::Build::new()
        .no_default_flags(true)
        .flag("-pedantic")
        .extra_warnings(false)
        .define("UNIX_HOST", None)
        .define("VER", Some("\"2.1\""))
        .define("NO_FP", None)
        .opt_level(3)
        .files(files)
        .file("src/picoc/platform.c")
        .compile("picoc");
}
