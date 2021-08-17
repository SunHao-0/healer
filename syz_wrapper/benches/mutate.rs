use criterion::{criterion_group, criterion_main, Criterion};
use healer_core::{
    corpus::CorpusWrapper,
    gen::{self, set_prog_len_range, FAVORED_MIN_PROG_LEN},
    mutation::mutate,
    relation::Relation,
    target::Target,
};
use rand::prelude::*;
use syz_wrapper::sys::{load_sys_target, SysTarget};

pub fn bench_prog_mutation(c: &mut Criterion) {
    let target = load_sys_target(SysTarget::LinuxAmd64).unwrap();
    let relation = Relation::new(&target);
    let mut rng = SmallRng::from_entropy();
    let corpus = dummy_corpus(&target, &relation, &mut rng);

    c.bench_function("prog-mutation", |b| {
        b.iter(|| {
            let mut p = corpus.select_one(&mut rng).unwrap();
            mutate(&target, &relation, &corpus, &mut rng, &mut p);
        })
    });
}

fn dummy_corpus(target: &Target, relation: &Relation, rng: &mut SmallRng) -> CorpusWrapper {
    let corpus = CorpusWrapper::new();
    let n = rng.gen_range(512..=4096);
    set_prog_len_range(3..8); // progs in corpus are always shorter
    for _ in 0..n {
        let prio = rng.gen_range(64..=1024);
        corpus.add_prog(gen::gen_prog(target, relation, rng), prio);
    }
    set_prog_len_range(FAVORED_MIN_PROG_LEN..FAVORED_MIN_PROG_LEN); // restore
    corpus
}

criterion_group!(benches, bench_prog_mutation);
criterion_main!(benches);
