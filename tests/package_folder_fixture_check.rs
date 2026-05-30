//! Validates that `fixtures/package_folder_mm.rs` compiles and
//! that the pre-graph-builder logic is correct.
//!
//! The actual case tests (1a/1b) include the same fixture
//! separately and use `build_fig3a_graph()`.

#[path = "fixtures/package_folder_mm.rs"]
mod package_folder_mm;

use package_folder_mm::build_fig3a_graph;

#[test]
fn fixture_compiles_and_builds_expected_l_side() {
    let (g, snap) = build_fig3a_graph();
    // Opt.1 structure: L-side only, 3 baseline nodes + 2 edges
    assert_eq!(snap.ids.len(), 3);
    assert_eq!(g.iter_nodes().count(), 3);
    assert_eq!(g.iter_edges().len(), 2);
}
