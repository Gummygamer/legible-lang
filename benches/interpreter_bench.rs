use criterion::{criterion_group, criterion_main, Criterion};

fn bench_fizzbuzz(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/fizzbuzz.clar");
    c.bench_function("fizzbuzz", |b| {
        b.iter(|| clarity_lang::run_source(source).unwrap())
    });
}

fn bench_hello(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/hello.clar");
    c.bench_function("hello", |b| {
        b.iter(|| clarity_lang::run_source(source).unwrap())
    });
}

criterion_group!(benches, bench_fizzbuzz, bench_hello);
criterion_main!(benches);
