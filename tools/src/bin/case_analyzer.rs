use fuzzer::report::FailedCase;
use std::collections::HashMap;
use std::env;
use std::fs::read;

fn main() {
    let f = env::args().nth(1).unwrap();
    let cases = read(&f).unwrap();

    let cases: Vec<FailedCase> = serde_json::from_slice(&cases).unwrap();
    let mut reasons = HashMap::new();

    for case in cases.into_iter() {
        let r = case.reason.split(':').collect::<Vec<_>>();
        if !r.is_empty() {
            let reason = String::from(r.last().unwrap().trim());
            if reason.contains("unknown type size") {
                println!("{}", case.p);
            }
            if reason.contains("too many arguments to function") && !case.p.contains("setpgrp") {
                println!("{}", case.p);
            }
            if reason.contains("too few arguments to function") {
                println!("{}", case.p);
            }
            let e = reasons.entry(reason).or_insert(0);
            *e += 1;
        }
    }

    let mut total = 0;
    let mut reasons = reasons.into_iter().collect::<Vec<_>>();
    reasons.sort();
    for (e, count) in reasons.into_iter() {
        total += count;
        println!("{} : {}", e, count);
    }
    println!("===== Total {}", total);
}
