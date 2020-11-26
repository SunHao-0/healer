use criterion::{criterion_group, criterion_main, Criterion};
use hl_fuzzer::fuzz::ValuePool;
use hl_fuzzer::gen::gen;
use hl_fuzzer::target::Target;

pub fn bench_gen(c: &mut Criterion) {
    let (sys, ty) = syscalls::syscalls();
    let target = Target::new(sys, ty, syscalls::REVISION);
    let pool = ValuePool::default();
    c.bench_function("Gen", |b| b.iter(|| gen(&target, &pool)));
}

criterion_group!(benches, bench_gen);
criterion_main!(benches);
