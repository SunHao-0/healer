use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use hl_fuzzer::exec::serialize::serialize;
use hl_fuzzer::fuzz::ValuePool;
use hl_fuzzer::gen::gen;
use hl_fuzzer::target::Target;

pub fn bench_gen(c: &mut Criterion) {
    let (sys, ty) = syscalls::syscalls();
    let target = Target::new(sys, ty, syscalls::REVISION);
    let pool = ValuePool::default();
    c.bench_function("Gen", |b| b.iter(|| gen(&target, &pool)));
}

// Avoid stack overflow.
static mut BUF: [u8; 4 << 20] = [0; 4 << 20];

pub fn bench_serialize(c: &mut Criterion) {
    let (sys, ty) = syscalls::syscalls();
    let target = Target::new(sys, ty, syscalls::REVISION);
    let pool = ValuePool::default();

    c.bench_function("Serialize", |b| {
        b.iter_batched(
            || gen(&target, &pool),
            |p| serialize(&target, p, unsafe { BUF.as_mut() }),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(benches, bench_gen, bench_serialize);
criterion_main!(benches);
