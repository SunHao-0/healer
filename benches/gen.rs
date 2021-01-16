use criterion::{criterion_group, criterion_main, Criterion};
use healer::fuzz::ValuePool;
use healer::gen::gen;
use healer::targets::Target;

pub fn bench_gen(c: &mut Criterion) {
    let (sys, ty) = todo!();
    let target = Target::new(sys, ty);
    let pool = ValuePool::default();
    c.bench_function("Gen", |b| b.iter(|| gen(&target, &pool)));
}

criterion_group!(benches, bench_gen);
criterion_main!(benches);
