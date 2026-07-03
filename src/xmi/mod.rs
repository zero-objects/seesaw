//! XMI import/export — EMF interoperability.
//!
//! Pragmatic XMI 2.0 subset parser/writer:
//!
//! - **Containment:** nested elements become edges whose `type_id` is
//!   the XML tag name of the child element (in EMF this corresponds to
//!   the name of the containment feature).
//! - **Node type:** `xmi:type` or `xsi:type`. If both are missing, the
//!   XML tag name is used as the type (only meaningful for the root
//!   element).
//! - **Identity:** `xmi:id` → `GhostId::from_opaque`. Elements without
//!   `xmi:id` receive synthetic path-based IDs
//!   (`"$path/<tag>[<idx>]"`) — stable across re-import.
//! - **Attributes:** all XML attributes except the XMI namespace and
//!   `xmi:type`/`xsi:type` are taken over as node attributes.
//! - **Cross-refs:** child elements with `xmi:idref` or `href="#<id>"`
//!   become cross-edges (their tag becomes the edge type), without
//!   creating a new node.
//!
//! This module is deliberately not a full EMF resource loader — it
//! covers the portion sufficient for the TGG benchmark against
//! Papyrus UML models (class-diagram structures, containment,
//! `href`/`idref`). More complex EMF features (profile applications,
//! multi-file references across resource URIs) are added on demand;
//! until then the EMF adapter is the preferred path.

use crate::graph::{GhostId, Status, TypedGraph};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::collections::BTreeMap;
use std::io::Cursor;
use thiserror::Error;

// ══ Errors ══════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum XmiError {
    #[error("XML parser error: {0}")]
    Xml(#[from] quick_xml::Error),
    // quick-xml 0.41 surfaces reader/writer I/O (and attribute unescape) errors
    // as std::io::Error rather than folding them into quick_xml::Error.
    #[error("XML I/O error: {0}")]
    XmlIo(#[from] std::io::Error),
    #[error("Attribute error: {0}")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),
    #[error("UTF-8 in XMI: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("invalid XMI: {0}")]
    Invalid(String),
}

pub type XmiResult<T> = Result<T, XmiError>;

// ══ Import ═══════════════════════════════════════════════════════════════

const CONTAINMENT_PREFIX: &str = "xmi_path:";

/// Parses an XMI string into a `TypedGraph` with SOLID baseline.
///
/// All nodes and edges receive status `Solid`; the resulting graph
/// is suitable as a baseline for a Cascade.
pub fn import_xmi(xml: &str) -> XmiResult<TypedGraph> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut graph = TypedGraph::new();
    let mut stack: Vec<ParentFrame> = Vec::new();
    // Path counter for children without xmi:id (deterministic synthetic IDs).
    let mut path_counter: Vec<BTreeMap<String, usize>> = vec![BTreeMap::new()];

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf)? {
            Event::Start(ref e) => {
                handle_start(
                    e,
                    &mut graph,
                    &mut stack,
                    &mut path_counter,
                    /*self_closing=*/ false,
                )?;
            }
            Event::Empty(ref e) => {
                handle_start(
                    e,
                    &mut graph,
                    &mut stack,
                    &mut path_counter,
                    /*self_closing=*/ true,
                )?;
            }
            Event::End(_) => {
                stack.pop();
                path_counter.pop();
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    Ok(graph)
}

/// Context frame of an open parent element on the parsing stack.
struct ParentFrame {
    ghost_id: GhostId,
    /// Path segment in the synthetic ID space (for children without xmi:id).
    path: String,
}

/// Tuple form for [`read_element_attributes`]: (xmi:id, xmi:type/xsi:type,
/// remaining attributes, idref).
type ParsedXmiAttrs = (
    Option<String>,
    Option<String>,
    BTreeMap<String, String>,
    Option<String>,
);

/// Reads `xmi:id`, `xmi:type`/`xsi:type` and the remaining attributes.
fn read_element_attributes(elem: &BytesStart<'_>) -> XmiResult<ParsedXmiAttrs> {
    let mut xmi_id: Option<String> = None;
    let mut xmi_type: Option<String> = None;
    let mut idref: Option<String> = None;
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();

    for a in elem.attributes() {
        let a = a?;
        let key = std::str::from_utf8(a.key.as_ref())?.to_string();
        // quick-xml 0.41 deprecates `unescape_value` in favour of
        // `normalized_value`, but the latter additionally collapses `\t\r\n`
        // to spaces (XML attribute-value normalization). We must NOT do that:
        // verbatim attribute payloads (e.g. Story-method bodies carrying
        // `&#10;`-encoded newlines) have to round-trip byte-for-byte. So we
        // deliberately keep pure entity-unescaping.
        #[allow(deprecated)]
        let val = a.unescape_value()?.to_string();

        match key.as_str() {
            "xmi:id" => xmi_id = Some(val),
            "xmi:type" | "xsi:type" => xmi_type = Some(val),
            "xmi:idref" => idref = Some(val),
            "href" => idref = Some(val.trim_start_matches('#').to_string()),
            k if k.starts_with("xmlns") || k.starts_with("xmi:") || k.starts_with("xsi:") => {
                // Namespace declarations and XMI metadata are ignored.
            }
            _ => {
                attrs.insert(key, val);
            }
        }
    }
    Ok((xmi_id, xmi_type, attrs, idref))
}

fn handle_start(
    elem: &BytesStart<'_>,
    graph: &mut TypedGraph,
    stack: &mut Vec<ParentFrame>,
    path_counter: &mut Vec<BTreeMap<String, usize>>,
    self_closing: bool,
) -> XmiResult<()> {
    let tag = std::str::from_utf8(elem.name().as_ref())?.to_string();

    // `xmi:XMI` is the envelope: no node creation, only a stack frame
    // so that its children become roots.
    if tag == "xmi:XMI" {
        if !self_closing {
            stack.push(ParentFrame {
                ghost_id: GhostId::from_baseline("__xmi_envelope__"),
                path: CONTAINMENT_PREFIX.to_string(),
            });
            path_counter.push(BTreeMap::new());
        }
        return Ok(());
    }

    let (xmi_id, xmi_type, attrs, idref) = read_element_attributes(elem)?;

    // Cross-ref: child element with idref/href → edge only, no node.
    if let Some(target_opaque) = idref.as_ref() {
        if let Some(parent) = stack.last() {
            let target = GhostId::from_opaque(target_opaque);
            let _ = graph.add_edge(
                parent.ghost_id,
                target,
                &tag,
                BTreeMap::new(),
                Status::Solid,
            );
        }
        if !self_closing {
            // Virtual frame so that the matching end tag pops cleanly.
            stack.push(ParentFrame {
                ghost_id: GhostId::from_baseline("__xmi_void__"),
                path: String::new(),
            });
            path_counter.push(BTreeMap::new());
        }
        return Ok(());
    }

    // Path index: deterministic numbering of the current tag in the
    // sibling context of the current frame.
    let counters = path_counter
        .last_mut()
        .ok_or_else(|| XmiError::Invalid("path stack empty".into()))?;
    let idx = counters.entry(tag.clone()).or_insert(0);
    let my_path_segment = format!("{}[{}]", tag, *idx);
    *idx += 1;

    let parent_path = stack
        .last()
        .map(|p| p.path.clone())
        .unwrap_or_else(|| CONTAINMENT_PREFIX.to_string());
    let my_path = format!("{parent_path}/{my_path_segment}");

    // Determine Ghost-ID: xmi:id preferred, otherwise path hash.
    let opaque = xmi_id.clone().unwrap_or_else(|| my_path.clone());
    let ghost_id = GhostId::from_opaque(&opaque);

    let type_id = xmi_type.unwrap_or_else(|| tag.clone());

    // Create node if not already present.
    if graph.get_node(&ghost_id).is_none() {
        graph.insert_node_data(crate::graph::NodeData {
            id: ghost_id,
            type_id,
            attrs,
            status: Status::Solid,
        });
    }

    // Containment edge from parent to this node.
    if let Some(parent) = stack.last() {
        let _ = graph.add_edge(
            parent.ghost_id,
            ghost_id,
            &tag,
            BTreeMap::new(),
            Status::Solid,
        );
    }

    if !self_closing {
        stack.push(ParentFrame {
            ghost_id,
            path: my_path,
        });
        path_counter.push(BTreeMap::new());
    }
    Ok(())
}

// ══ Export ══════════════════════════════════════════════════════════════

/// Serializes a `TypedGraph` back to XMI.
///
/// Strategy: nodes without an incoming containment edge are emitted as
/// roots, all others recursively along the containment edges. The key
/// `type_id` is emitted as `xmi:type`; the tag name is the containment
/// feature (edge type) — for roots this is a synthetic `xmi:XMI`
/// envelope.
pub fn export_xmi(graph: &TypedGraph) -> XmiResult<String> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

    let mut writer = Writer::new(Cursor::new(&mut buf));

    let mut envelope = BytesStart::new("xmi:XMI");
    envelope.push_attribute(("xmi:version", "2.0"));
    envelope.push_attribute(("xmlns:xmi", "http://www.omg.org/XMI"));
    envelope.push_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
    writer.write_event(Event::Start(envelope.clone()))?;

    // Roots: nodes without incoming containment edges (and matchable).
    let incoming: std::collections::HashSet<GhostId> = graph
        .iter_edges()
        .into_iter()
        .filter(|(_, _, e)| e.status.is_matchable())
        .map(|(_, tgt, _)| tgt)
        .collect();

    let roots: Vec<GhostId> = graph
        .matchable_nodes()
        .filter(|n| !incoming.contains(&n.id))
        .map(|n| n.id)
        .collect();

    for root in roots {
        // For roots we use the type as the tag (EMF convention:
        // `<uml:Model ...>` instead of `<packagedElement xmi:type="uml:Model">`).
        let root_tag = graph
            .get_node(&root)
            .map(|n| n.type_id.clone())
            .unwrap_or_else(|| "element".to_string());
        write_node_recursive(&mut writer, graph, &root, &root_tag, &mut Vec::new())?;
    }

    writer.write_event(Event::End(quick_xml::events::BytesEnd::new("xmi:XMI")))?;
    String::from_utf8(buf).map_err(|e| XmiError::Invalid(e.to_string()))
}

fn write_node_recursive<W: std::io::Write>(
    writer: &mut Writer<W>,
    graph: &TypedGraph,
    id: &GhostId,
    tag: &str,
    visited: &mut Vec<GhostId>,
) -> XmiResult<()> {
    if visited.contains(id) {
        // Cycle (should not happen for containment) — emit as a
        // cross-ref.
        let mut cr = BytesStart::new(tag);
        cr.push_attribute(("xmi:idref", id.short().as_str()));
        writer.write_event(Event::Empty(cr))?;
        return Ok(());
    }
    let Some(node) = graph.get_node(id) else {
        return Ok(());
    };
    visited.push(*id);

    let mut elem = BytesStart::new(tag);
    elem.push_attribute(("xmi:id", node.id.short().as_str()));
    elem.push_attribute(("xmi:type", node.type_id.as_str()));
    for (k, v) in &node.attrs {
        elem.push_attribute((k.as_str(), v.as_str()));
    }

    let children: Vec<(GhostId, String)> = graph
        .outgoing_edges(id)
        .into_iter()
        .map(|(e, tgt)| (tgt, e.type_id.clone()))
        .collect();

    if children.is_empty() {
        writer.write_event(Event::Empty(elem))?;
    } else {
        writer.write_event(Event::Start(elem.clone()))?;
        for (child_id, edge_type) in children {
            if visited.contains(&child_id) {
                let mut cr = BytesStart::new(&edge_type);
                cr.push_attribute(("xmi:idref", child_id.short().as_str()));
                writer.write_event(Event::Empty(cr))?;
            } else {
                write_node_recursive(writer, graph, &child_id, &edge_type, visited)?;
            }
        }
        writer.write_event(Event::End(quick_xml::events::BytesEnd::new(tag)))?;
    }
    visited.pop();
    Ok(())
}

// ══ Tests ═══════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_UML_XMI: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmi:XMI xmi:version="2.0" xmlns:xmi="http://www.omg.org/XMI"
         xmlns:uml="http://www.eclipse.org/uml2/5.0.0/UML">
  <uml:Model xmi:id="_root" name="TestModel">
    <packagedElement xmi:type="uml:Class" xmi:id="_c1" name="Person">
      <ownedAttribute xmi:id="_a1" name="age" />
    </packagedElement>
    <packagedElement xmi:type="uml:Class" xmi:id="_c2" name="Address" />
  </uml:Model>
</xmi:XMI>
"#;

    #[test]
    fn import_extracts_nodes_and_containment() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        // Nodes: XMI root, Model, 2 classes, 1 attribute = 5 (when xmi:XMI
        // is also counted as a node).
        assert!(
            g.node_count() >= 4,
            "at least 4 nodes, got {}",
            g.node_count()
        );

        // xmi:id "_c1" → Person
        let c1 = GhostId::from_opaque("_c1");
        let n = g.get_node(&c1).expect("Person exists");
        assert_eq!(n.type_id, "uml:Class");
        assert_eq!(n.attrs.get("name").map(String::as_str), Some("Person"));
    }

    #[test]
    fn import_preserves_attributes() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let root = GhostId::from_opaque("_root");
        let n = g.get_node(&root).expect("Model exists");
        assert_eq!(n.attrs.get("name").map(String::as_str), Some("TestModel"));
    }

    #[test]
    fn import_creates_containment_edges() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let root = GhostId::from_opaque("_root");
        let c1 = GhostId::from_opaque("_c1");
        let c2 = GhostId::from_opaque("_c2");
        assert!(
            g.has_edge_between(&root, &c1, "packagedElement"),
            "Model → Person containment missing"
        );
        assert!(
            g.has_edge_between(&root, &c2, "packagedElement"),
            "Model → Address containment missing"
        );
    }

    #[test]
    fn roundtrip_preserves_structure() {
        let g1 = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let xml = export_xmi(&g1).expect("export ok");
        // Not bit-identical, but re-importable with the same number of
        // matchable nodes (IDs change due to short() form, but the
        // structure is preserved).
        let g2 = import_xmi(&xml).expect("re-import ok");
        assert_eq!(
            g1.matchable_nodes().count(),
            g2.matchable_nodes().count(),
            "node count changed: before={}, after={}",
            g1.matchable_nodes().count(),
            g2.matchable_nodes().count()
        );
    }

    #[test]
    fn import_handles_cross_reference_via_idref() {
        let xmi_with_ref = r#"<?xml version="1.0" encoding="UTF-8"?>
<xmi:XMI xmi:version="2.0" xmlns:xmi="http://www.omg.org/XMI">
  <uml:Model xmi:id="_root" name="Test">
    <packagedElement xmi:type="uml:Class" xmi:id="_c1" name="A" />
    <packagedElement xmi:type="uml:Class" xmi:id="_c2" name="B">
      <superClass xmi:idref="_c1" />
    </packagedElement>
  </uml:Model>
</xmi:XMI>
"#;
        let g = import_xmi(xmi_with_ref).expect("import ok");
        let c1 = GhostId::from_opaque("_c1");
        let c2 = GhostId::from_opaque("_c2");
        assert!(
            g.has_edge_between(&c2, &c1, "superClass"),
            "B → A superClass reference missing"
        );
    }

    #[test]
    fn empty_xmi_yields_empty_graph() {
        let empty = r#"<?xml version="1.0"?><xmi:XMI xmlns:xmi="http://www.omg.org/XMI"/>"#;
        let g = import_xmi(empty).expect("empty import ok");
        assert_eq!(g.node_count(), 0);
    }

    /// Case 11 (Sirius EdgeMapping xsi:type): the XMI writer emits
    /// `xmi:type` for every node, which Sirius uses on read as a
    /// subtype hint. Sirius accepts both `xmi:type` and `xsi:type` —
    /// both are defined as equivalent in the EMF XMI spec.
    #[test]
    fn case11_export_writes_xmi_type_for_each_node() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let exported = export_xmi(&g).expect("export ok");
        // The writer emits `xmi:type` for every node — not
        // `xsi:type`, but semantically equivalent in EMF.
        assert!(
            exported.contains("xmi:type=\"uml:Class\"")
                || exported.contains("xmi:type=\"uml:Model\""),
            "export must emit xmi:type attribute for nodes.\nExport:\n{exported}"
        );
    }

    /// Case 11 roundtrip: import → export → import preserves the
    /// EClass type information (uml:Class etc.). If the writer were
    /// to omit xmi:type, the second import would read the nodes as
    /// the generic tag-name type instead of the specific EClass —
    /// that is exactly the Sirius EdgeMapping symptom.
    #[test]
    fn case11_roundtrip_preserves_eclass_type() {
        let g1 = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let class_count_pre = g1.iter_nodes().filter(|n| n.type_id == "uml:Class").count();
        let exported = export_xmi(&g1).expect("export ok");
        let g2 = import_xmi(&exported).expect("re-import ok");
        let class_count_post = g2.iter_nodes().filter(|n| n.type_id == "uml:Class").count();
        assert_eq!(
            class_count_pre, class_count_post,
            "uml:Class node count must be preserved across roundtrip \
             (pre={class_count_pre}, post={class_count_post})"
        );
        assert!(class_count_post >= 2, "at least 2 classes expected");
    }
}
