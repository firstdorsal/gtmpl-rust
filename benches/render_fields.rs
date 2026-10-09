use std::collections::HashMap;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use gtmpl_ng::{Context, Template, Value};

fn context(fields: usize) -> Context {
    let mut metadata = HashMap::with_capacity(fields + 1);
    metadata.insert("name".to_owned(), Value::from("example"));
    for index in 0..fields {
        metadata.insert(
            format!("field{index}"),
            Value::from(format!("metadata-value-{index}")),
        );
    }

    let root: HashMap<String, Value> = [
        ("kind".to_owned(), Value::from("Deployment")),
        ("metadata".to_owned(), Value::from(metadata)),
    ]
    .into();
    Context::from(root)
}

fn render_fields(c: &mut Criterion) {
    let mut template = Template::default();
    template
        .parse("{{.kind}}-{{.metadata.name}}")
        .expect("a valid template");

    let mut group = c.benchmark_group("render_fields");
    for fields in [10usize, 100, 1_000] {
        let context = context(fields);
        group.bench_with_input(BenchmarkId::from_parameter(fields), &fields, |b, _| {
            b.iter(|| black_box(template.render(black_box(&context)).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, render_fields);
criterion_main!(benches);
