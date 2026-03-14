use criterion::{criterion_group, criterion_main, Criterion};

fn bench_fizzbuzz(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/fizzbuzz.lbl");
    c.bench_function("fizzbuzz", |b| {
        b.iter(|| legible_lang::run_source(source).unwrap())
    });
}

fn bench_hello(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/hello.lbl");
    c.bench_function("hello", |b| {
        b.iter(|| legible_lang::run_source(source).unwrap())
    });
}

criterion_group!(benches, bench_fizzbuzz, bench_hello);
criterion_main!(benches);
