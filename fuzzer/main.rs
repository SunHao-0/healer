use hl_fuzzer::fuzz::ValuePool;
use hl_fuzzer::gen::gen;
use hl_fuzzer::target::Target;

pub fn main() {
    let (sys, ty) = syscalls::syscalls();
    let target = Target::new(sys, ty, syscalls::REVISION);
    let pool = ValuePool::default();
    let p = gen(&target, &pool);
    println!("{}", p);
}
