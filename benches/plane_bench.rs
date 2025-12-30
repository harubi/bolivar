use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::hash::{Hash, Hasher};
use std::hint::black_box;

// Minimal bbox item for benchmarking
#[derive(Clone, Debug)]
struct BBoxItem {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    id: usize,
}

impl BBoxItem {
    fn new(x0: f64, y0: f64, x1: f64, y1: f64, id: usize) -> Self {
        Self { x0, y0, x1, y1, id }
    }
}

impl PartialEq for BBoxItem {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for BBoxItem {}

impl Hash for BBoxItem {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

// Implement the HasBBox trait from bolivar
impl bolivar::utils::HasBBox for BBoxItem {
    fn x0(&self) -> f64 {
        self.x0
    }
    fn y0(&self) -> f64 {
        self.y0
    }
    fn x1(&self) -> f64 {
        self.x1
    }
    fn y1(&self) -> f64 {
        self.y1
    }
    fn bbox(&self) -> bolivar::utils::Rect {
        (self.x0, self.y0, self.x1, self.y1)
    }
}

/// Generate N items scattered across a page (simulating text layout)
fn generate_items(n: usize) -> Vec<BBoxItem> {
    let page_width = 612.0; // US Letter width in points
    let page_height = 792.0; // US Letter height in points

    (0..n)
        .map(|i| {
            // Distribute items in a grid-like pattern
            let row = i / 10;
            let col = i % 10;
            let x0 = (col as f64) * 60.0 + 10.0;
            let y0 = (row as f64) * 12.0 + 10.0;
            let width = 50.0 + (i % 3) as f64 * 10.0;
            let height = 10.0;
            BBoxItem::new(
                x0.min(page_width - width),
                y0.min(page_height - height),
                (x0 + width).min(page_width),
                (y0 + height).min(page_height),
                i,
            )
        })
        .collect()
}

fn bench_construct(c: &mut Criterion) {
    let mut group = c.benchmark_group("plane_construct");

    for size in [100, 1_000, 10_000] {
        let items = generate_items(size);
        let page_bbox = (0.0, 0.0, 612.0, 792.0);

        group.bench_with_input(BenchmarkId::from_parameter(size), &items, |b, items| {
            b.iter(|| {
                let mut plane = bolivar::utils::Plane::new(page_bbox, 50);
                plane.extend(black_box(items.clone()));
                plane
            })
        });
    }
    group.finish();
}

fn bench_find(c: &mut Criterion) {
    let mut group = c.benchmark_group("plane_find");

    for size in [100, 1_000, 10_000] {
        let items = generate_items(size);
        let page_bbox = (0.0, 0.0, 612.0, 792.0);
        let mut plane = bolivar::utils::Plane::new(page_bbox, 50);
        plane.extend(items);

        // Query bbox: roughly 1/4 of the page
        let query_bbox = (100.0, 100.0, 400.0, 500.0);

        group.bench_with_input(BenchmarkId::from_parameter(size), &plane, |b, plane| {
            b.iter(|| plane.find(black_box(query_bbox)))
        });
    }
    group.finish();
}

fn bench_remove(c: &mut Criterion) {
    let mut group = c.benchmark_group("plane_remove");

    for size in [100, 1_000, 10_000] {
        let items = generate_items(size);
        let page_bbox = (0.0, 0.0, 612.0, 792.0);

        // Pick an item in the middle to remove
        let target = items[size / 2].clone();

        group.bench_with_input(
            BenchmarkId::from_parameter(size),
            &(items, target),
            |b, (items, target)| {
                b.iter_batched(
                    || {
                        let mut plane = bolivar::utils::Plane::new(page_bbox, 50);
                        plane.extend(items.clone());
                        plane
                    },
                    |mut plane| plane.remove(black_box(target)),
                    criterion::BatchSize::SmallInput,
                )
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_construct, bench_find, bench_remove);
criterion_main!(benches);
