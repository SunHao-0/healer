use fuzzer::report::*;
use std::fmt::Write;
use std::fs::create_dir_all;
use std::fs::read;
use std::fs::write;
use std::path::PathBuf;
use std::process::exit;
use structopt::StructOpt;

const BOOK_TOML: &str = r#"[book]
title = "Healer Report"
authors = ["Healer"]
description = "Report of fuzzing result"

[output.html.fold]
enable = true
level = 0
"#;

#[derive(Debug, StructOpt)]
#[structopt(name = "repot")]
struct Settings {
    #[structopt(short = "c", long = "crash")]
    crashes: Option<Vec<PathBuf>>,
    #[structopt(short = "n", long = "normal")]
    normal: PathBuf,
    #[structopt(short = "f", long = "failed")]
    failed: Option<PathBuf>,
    #[structopt(short = "o", long = "out")]
    out: PathBuf,
}

fn main() {
    let settings = Settings::from_args();
    let mut summary = String::new();
    let mut crashes_mds = vec![(String::from("crash.md"), String::new())];
    let mut failed_mds = vec![(String::from("failed.md"), String::new())];
    let mut normal_mds = vec![(String::from("normal.md"), String::new())];

    writeln!(summary, "# Summary").unwrap();

    writeln!(summary, "- [Crashes](crash/crash.md)").unwrap();

    if let Some(crashes) = settings.crashes {
        for crash in crashes.into_iter() {
            let crash = read(&crash).unwrap_or_else(|e| {
                eprintln!("Fail to read {:?}: {}", crash, e);
                exit(1);
            });
            let crash: CrashedCase = serde_json::from_slice(&crash).unwrap_or_else(|e| {
                eprintln!("Fail to deserialize: {}", e);
                exit(1);
            });
            let crash_md = report_crash(&crash);
            let path = format!("{}.md", crash.meta.title);

            writeln!(summary, "    - [{}](carsh/{})", crash.meta.title, path).unwrap();
            crashes_mds.push((path, crash_md));
        }
    }

    writeln!(summary, "- [Failed](failed/failed.md)").unwrap();
    if let Some(failed) = settings.failed {
        let failed = read(&failed).unwrap_or_else(|e| {
            eprintln!("Fail to read {:?}: {}", failed, e);
            exit(1);
        });
        let failed_cases: Vec<FailedCase> = serde_json::from_slice(&failed).unwrap_or_else(|e| {
            eprintln!("Fail to deserialize: {}", e);
            exit(1);
        });
        for case in failed_cases.into_iter() {
            let failed_md = report_failed(&case);
            let path = format!("{}.md", case.meta.title);

            writeln!(summary, "    - [{}](failed/{})", case.meta.title, path).unwrap();
            failed_mds.push((path, failed_md));
        }
    }

    let normal_path = settings.normal;
    writeln!(summary, "- [Normal](normal/normal.md)").unwrap();
    let normal_cases = read(&normal_path).unwrap_or_else(|e| {
        eprintln!("Fail to read {:?}: {}", normal_path, e);
        exit(1);
    });
    let normal_cases: Vec<ExecutedCase> =
        serde_json::from_slice(&normal_cases).unwrap_or_else(|e| {
            eprintln!("Fail to deserialize: {}", e);
            exit(1);
        });
    for case in normal_cases.into_iter() {
        let normal_case_md = report_normal(&case);
        let path = format!("{}.md", case.meta.title);

        writeln!(summary, "    - [{}](normal/{})", case.meta.title, path).unwrap();
        normal_mds.push((path, normal_case_md));
    }

    let mut out: PathBuf = settings.out;
    out.push("src");
    create_dir_all(&mut out).unwrap();
    persist(&out, "SUMMARY.md", summary);

    out.push("crash");
    create_dir_all(&out).unwrap();
    for (f_name, crash) in crashes_mds.into_iter() {
        persist(&out, &f_name, crash);
    }
    out.pop();

    out.push("failed");
    create_dir_all(&out).unwrap();
    for (f_name, failed) in failed_mds.into_iter() {
        persist(&out, &f_name, failed);
    }
    out.pop();

    out.push("normal");
    create_dir_all(&out).unwrap();
    for (f_name, normal) in normal_mds.into_iter() {
        persist(&out, &f_name, normal);
    }
    out.pop();

    out.pop();
    out.push("book.toml");
    write(&out, BOOK_TOML).unwrap();
}

fn report_crash(crash: &CrashedCase) -> String {
    let mut buf = String::new();
    writeln!(buf, "# {}", crash.meta.title).unwrap();
    writeln!(buf, "**Id**:   {}</br>", crash.meta.id).unwrap();
    writeln!(buf, "**Repo**: {}</br>", crash.repo).unwrap();
    writeln!(buf, "**Test Time**: {}</br>", crash.meta.test_time).unwrap();
    writeln!(buf, "## {}", "Prog").unwrap();
    writeln!(buf, "``` c").unwrap();
    for line in crash.p.lines() {
        writeln!(buf, "{}", line).unwrap();
    }
    writeln!(buf, "```").unwrap();
    writeln!(buf, "## *Crash*").unwrap();
    for line in crash.crash.to_string().lines() {
        writeln!(buf, "{}</br>", line).unwrap();
    }
    buf
}

fn report_failed(failed: &FailedCase) -> String {
    let mut buf = String::new();
    writeln!(buf, "# {}", failed.meta.title).unwrap();
    writeln!(buf, "**Id**:   {}</br>", failed.meta.id).unwrap();
    writeln!(buf, "**Test Time**: {}</br>", failed.meta.test_time).unwrap();
    writeln!(buf, "## {}", "Prog").unwrap();
    writeln!(buf, "``` c").unwrap();
    for line in failed.p.lines() {
        writeln!(buf, "{}", line).unwrap();
    }
    writeln!(buf, "```").unwrap();
    writeln!(buf, "## *Reason*").unwrap();
    for line in failed.reason.lines() {
        writeln!(buf, "{}</br>", line).unwrap();
    }
    buf
}

fn report_normal(normal: &ExecutedCase) -> String {
    let mut buf = String::new();
    writeln!(buf, "# {}", normal.meta.title).unwrap();
    writeln!(buf, "**Id**:   {}</br>", normal.meta.id).unwrap();
    writeln!(buf, "**Test Time**: {}</br>", normal.meta.test_time).unwrap();
    writeln!(buf, "## {}", "Prog").unwrap();
    writeln!(buf, "``` c").unwrap();
    for line in normal.p.lines() {
        writeln!(buf, "{}", line).unwrap();
    }
    writeln!(buf, "```").unwrap();
    writeln!(buf, "## *Feedback*").unwrap();
    writeln!(buf, "Block:  {:?}</br>", normal.block_num).unwrap();
    writeln!(buf, "Branch: {:?}</br>", normal.branch_num).unwrap();
    writeln!(buf, "New Block:  {}</br>", normal.new_block).unwrap();
    writeln!(buf, "New Branch: {}</br>", normal.new_branch).unwrap();
    buf
}

fn persist(dir: &PathBuf, f_name: &str, content: String) {
    let mut p = dir.clone();
    p.push(f_name);
    write(p, content).unwrap()
}
