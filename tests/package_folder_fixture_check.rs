//! Validiert, dass `fixtures/package_folder_mm.rs` kompiliert und
//! die Pre-Graph-Builder-Logik korrekt ist.
//!
//! Die eigentlichen Case-Tests (1a/1b) binden dieselbe Fixture
//! separat ein und nutzen `build_fig3a_graph()`.

#[path = "fixtures/package_folder_mm.rs"]
mod package_folder_mm;

use package_folder_mm::build_fig3a_graph;

#[test]
fn fixture_compiles_and_builds_expected_l_side() {
    let (g, snap) = build_fig3a_graph();
    // Opt.1-Struktur: nur L-Seite, 3 Baseline-Nodes + 2 Edges
    assert_eq!(snap.ids.len(), 3);
    assert_eq!(g.iter_nodes().count(), 3);
    assert_eq!(g.iter_edges().len(), 2);
}
