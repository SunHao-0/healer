use hl_fuzzer::fuzz::*;
use hl_fuzzer::gen::*;
use hl_fuzzer::target::*;

pub fn main() {
    // Parse command line arguments, validate thems
    // Extract env vars
    // Maybe add some performance operations, such as bind cpu
    // start the fuzz instance.
    let target = Target::new();
    let pool = ValuePool::default();
    let p = gen(&target, &pool);
    println!("{}", p);
}
