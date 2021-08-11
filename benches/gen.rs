use criterion::{criterion_group, criterion_main, Criterion};
use healer::gen::gen;
use healer::targets::Target;
use rustc_hash::FxHashSet;

pub fn bench_gen(c: &mut Criterion) {
    let target = Target::new("linux/amd64", &FxHashSet::default()).unwrap();
    c.bench_function("Gen", |b| b.iter(|| gen(&target)));
}

criterion_group!(benches, bench_gen);
criterion_main!(benches);
