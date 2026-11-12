//! Manual micro-benchmarks. Run with:
//! `cargo test -p gloss-widgets --test bench -- --ignored --nocapture`

use std::time::Instant;

use gloss_core::Style;
use gloss_widgets::{Document, Node};

#[test]
#[ignore = "manual benchmark"]
fn clean_sync_thousand_nodes() {
    let mut doc = Document::new();
    for _ in 0..1_000 {
        doc.add(
            doc.root(),
            Node::panel(Style::new().width(10.0).height(10.0)),
        )
        .expect("add node");
    }
    assert_eq!(doc.sync(), 0);
    doc.clear_dirty();

    let start = Instant::now();
    for _ in 0..1_000 {
        assert_eq!(doc.sync(), 0);
    }
    let per = start.elapsed().as_secs_f64() * 1e6 / 1_000.0;
    eprintln!(
        "clean sync: {:.2} us per call (1,000 nodes, no bindings)",
        per
    );

    let start = Instant::now();
    for _ in 0..1_000 {
        assert_eq!(doc.dirty(), 0);
    }
    let per = start.elapsed().as_secs_f64() * 1e6 / 1_000.0;
    eprintln!("dirty check: {:.3} us per call", per);
}
