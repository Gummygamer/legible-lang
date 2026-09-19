use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn bench_fizzbuzz(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/fizzbuzz.lbl");
    c.bench_function("fizzbuzz", |b| {
        b.iter(|| legible_lang::run_source(source).unwrap())
    });
}

fn bench_fizzbuzz_prepared(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/fizzbuzz.lbl");
    let prepared = legible_lang::prepare_source(source).unwrap();
    c.bench_function("fizzbuzz_prepared_execution", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            prepared.run(black_box(&mut output)).unwrap();
            black_box(output);
        })
    });
}

fn bench_preparation(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/hello.lbl");
    c.bench_function("hello_preparation", |b| {
        b.iter(|| black_box(legible_lang::prepare_source(source).unwrap()))
    });
}

fn bench_hello(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/hello.lbl");
    c.bench_function("hello", |b| {
        b.iter(|| legible_lang::run_source(source).unwrap())
    });
}

criterion_group!(
    benches,
    bench_fizzbuzz,
    bench_fizzbuzz_prepared,
    bench_preparation,
    bench_hello
);
criterion_main!(benches);
