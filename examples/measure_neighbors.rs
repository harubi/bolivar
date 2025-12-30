//! Measure neighbor distribution in PDF text boxes for K_NEIGHBORS tuning
//!
//! Usage: cargo run --example measure_neighbors /path/to/file.pdf

use bolivar::error::Result;
use bolivar::high_level::{ExtractOptions, extract_pages};
use bolivar::layout::{LAParams, LTItem, LTPage};
use bolivar::utils::{HasBBox, Rect};
use std::collections::BinaryHeap;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: cargo run --example measure_neighbors <pdf_path>");
        std::process::exit(1);
    }

    let pdf_path = &args[1];
    let pdf_data = std::fs::read(pdf_path).expect("Failed to read PDF");

    let laparams = LAParams::default();
    let options = ExtractOptions {
        laparams: Some(laparams),
        ..Default::default()
    };

    let pages: Vec<LTPage> = extract_pages(&pdf_data, Some(options))?
        .filter_map(|r| r.ok())
        .collect();

    println!("=== Neighbor Analysis for {} ===\n", pdf_path);

    let mut total_boxes = 0;
    let mut total_merges = 0;
    let mut merge_ranks: Vec<usize> = Vec::new();

    for (page_num, page) in pages.iter().enumerate() {
        let boxes = collect_text_boxes(page);
        let n = boxes.len();
        if n < 2 {
            continue;
        }

        println!("Page {}: {} text boxes", page_num + 1, n);
        total_boxes += n;

        // Simulate clustering and track which neighbor rank each merge happens at
        let page_ranks = analyze_merge_ranks(&boxes);
        total_merges += page_ranks.len();
        merge_ranks.extend(page_ranks.iter());

        // Per-page stats
        if !page_ranks.is_empty() {
            let max_rank = *page_ranks.iter().max().unwrap();
            let avg_rank: f64 = page_ranks.iter().sum::<usize>() as f64 / page_ranks.len() as f64;
            println!(
                "  Merges: {}, Max rank: {}, Avg rank: {:.1}",
                page_ranks.len(),
                max_rank,
                avg_rank
            );
        }
    }

    println!("\n=== Summary ===");
    println!("Total pages: {}", pages.len());
    println!("Total text boxes: {}", total_boxes);
    println!("Total merges: {}", total_merges);

    if !merge_ranks.is_empty() {
        merge_ranks.sort();
        let max_rank = *merge_ranks.iter().max().unwrap();
        let avg_rank: f64 = merge_ranks.iter().sum::<usize>() as f64 / merge_ranks.len() as f64;
        let median_rank = merge_ranks[merge_ranks.len() / 2];
        let p90_idx = (merge_ranks.len() as f64 * 0.9) as usize;
        let p95_idx = (merge_ranks.len() as f64 * 0.95) as usize;
        let p99_idx = (merge_ranks.len() as f64 * 0.99) as usize;

        println!("\nMerge rank distribution:");
        println!("  Min: 1 (always the closest neighbor)");
        println!("  Max: {}", max_rank);
        println!("  Avg: {:.2}", avg_rank);
        println!("  Median (p50): {}", median_rank);
        println!("  p90: {}", merge_ranks.get(p90_idx).unwrap_or(&0));
        println!("  p95: {}", merge_ranks.get(p95_idx).unwrap_or(&0));
        println!("  p99: {}", merge_ranks.get(p99_idx).unwrap_or(&0));

        // Recommend K
        let recommended_k = merge_ranks.get(p95_idx).unwrap_or(&10).max(&5);
        println!("\n=== Recommendation ===");
        println!(
            "K_NEIGHBORS = {} would capture 95% of merges",
            recommended_k
        );

        // Distribution histogram
        println!("\nRank histogram:");
        for k in [1, 2, 3, 5, 10, 15, 20, 30, 50] {
            let count = merge_ranks.iter().filter(|&&r| r <= k).count();
            let pct = count as f64 / merge_ranks.len() as f64 * 100.0;
            println!("  K≤{:2}: {:>5} merges ({:>5.1}%)", k, count, pct);
        }
    }

    Ok(())
}

/// Collect all text boxes from a page
fn collect_text_boxes(page: &LTPage) -> Vec<TextBox> {
    let mut boxes = Vec::new();
    for item in page.iter() {
        collect_boxes_recursive(item, &mut boxes);
    }
    boxes
}

fn collect_boxes_recursive(item: &LTItem, boxes: &mut Vec<TextBox>) {
    match item {
        LTItem::TextBox(tb) => {
            boxes.push(TextBox {
                bbox: tb.bbox(),
                id: boxes.len(),
            });
        }
        LTItem::TextLine(_) | LTItem::Char(_) | LTItem::Anno(_) => {
            // Skip - we want boxes, not their children
        }
        LTItem::Figure(fig) => {
            for child in fig.iter() {
                collect_boxes_recursive(child, boxes);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug)]
struct TextBox {
    bbox: Rect,
    id: usize,
}

impl HasBBox for TextBox {
    fn x0(&self) -> f64 {
        self.bbox.0
    }
    fn y0(&self) -> f64 {
        self.bbox.1
    }
    fn x1(&self) -> f64 {
        self.bbox.2
    }
    fn y1(&self) -> f64 {
        self.bbox.3
    }
    fn bbox(&self) -> Rect {
        self.bbox
    }
}

/// Analyze what neighbor rank each merge happens at
fn analyze_merge_ranks(boxes: &[TextBox]) -> Vec<usize> {
    if boxes.len() < 2 {
        return vec![];
    }

    // Build full distance matrix to know true ranks
    let n = boxes.len();
    let mut distances: Vec<Vec<(f64, usize)>> = Vec::with_capacity(n);

    for i in 0..n {
        let mut row: Vec<(f64, usize)> = Vec::with_capacity(n);
        for j in 0..n {
            if i != j {
                let d = box_distance(&boxes[i], &boxes[j]);
                row.push((d, j));
            }
        }
        // Sort by distance to get neighbor ranks
        row.sort_by(|a, b| a.0.total_cmp(&b.0));
        distances.push(row);
    }

    // Simulate clustering - track which rank each merge occurs at
    let mut merge_ranks: Vec<usize> = Vec::new();
    let mut merged: Vec<bool> = vec![false; n];
    let mut remaining = n;

    // Use min-heap (Reverse for min)
    #[derive(PartialEq)]
    struct Entry {
        dist: f64,
        i: usize,
        j: usize,
    }
    impl Eq for Entry {}
    impl PartialOrd for Entry {
        fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(other))
        }
    }
    impl Ord for Entry {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            // Reverse for min-heap
            other.dist.total_cmp(&self.dist)
        }
    }

    let mut heap: BinaryHeap<Entry> = BinaryHeap::new();

    // Initialize heap with all pairs
    for i in 0..n {
        for j in (i + 1)..n {
            let d = box_distance(&boxes[i], &boxes[j]);
            heap.push(Entry { dist: d, i, j });
        }
    }

    while let Some(entry) = heap.pop() {
        if merged[entry.i] || merged[entry.j] {
            continue;
        }

        // Find what rank this neighbor was at for box i
        let rank_in_i = distances[entry.i]
            .iter()
            .position(|(_, idx)| *idx == entry.j)
            .map(|p| p + 1) // 1-indexed rank
            .unwrap_or(n);

        merge_ranks.push(rank_in_i);

        // Mark one as merged (simplified - real algo creates groups)
        merged[entry.j] = true;
        remaining -= 1;

        if remaining <= 1 {
            break;
        }
    }

    merge_ranks
}

/// Distance between two boxes (simplified - uses center distance)
fn box_distance(a: &TextBox, b: &TextBox) -> f64 {
    let ax = (a.bbox.0 + a.bbox.2) / 2.0;
    let ay = (a.bbox.1 + a.bbox.3) / 2.0;
    let bx = (b.bbox.0 + b.bbox.2) / 2.0;
    let by = (b.bbox.1 + b.bbox.3) / 2.0;

    let dx = ax - bx;
    let dy = ay - by;
    (dx * dx + dy * dy).sqrt()
}
