//! XMI-Import/Export — EMF-Interoperabilität.
//!
//! Pragmatischer XMI-2.0-Subset-Parser/-Writer:
//!
//! - **Containment:** geschachtelte Elemente werden zu Kanten, deren
//!   `type_id` der XML-Tag-Name des Kind-Elements ist (entspricht in EMF
//!   dem Namen des Containment-Features).
//! - **Knoten-Typ:** `xmi:type` bzw. `xsi:type`. Fehlt beides, wird der
//!   XML-Tag-Name als Typ genommen (nur sinnvoll für das Root-Element).
//! - **Identität:** `xmi:id` → `GhostId::from_opaque`. Elemente ohne
//!   `xmi:id` bekommen synthetische Pfad-basierte IDs
//!   (`"$path/<tag>[<idx>]"`) — stabil bei Re-Import.
//! - **Attribute:** alle XML-Attribute außer dem XMI-Namespace und
//!   `xmi:type`/`xsi:type` werden als Knoten-Attribute übernommen.
//! - **Cross-Refs:** Kind-Elemente mit `xmi:idref` oder `href="#<id>"`
//!   werden zu Cross-Edges (ihr Tag wird zum Kanten-Typ), ohne dass ein
//!   neuer Knoten entsteht.
//!
//! Das Modul ist bewusst kein vollständiger EMF-Resource-Loader — es
//! deckt den Anteil ab, der für den TGG-Benchmark gegen Papyrus-UML-
//! Modelle reicht (Class-Diagramm-Strukturen, Containment,
//! `href`/`idref`). Komplexere EMF-Features (profile applications,
//! multi-file references über Resource-URIs hinweg) werden bei Bedarf
//! ergänzt; bis dahin ist der EMF-Adapter der bevorzugte Pfad.

use crate::graph::{GhostId, Status, TypedGraph};
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use std::collections::BTreeMap;
use std::io::Cursor;
use thiserror::Error;

// ══ Fehler ══════════════════════════════════════════════════════════════

#[derive(Debug, Error)]
pub enum XmiError {
    #[error("XML-Parser-Fehler: {0}")]
    Xml(#[from] quick_xml::Error),
    #[error("Attribut-Fehler: {0}")]
    XmlAttr(#[from] quick_xml::events::attributes::AttrError),
    #[error("UTF-8 im XMI: {0}")]
    Utf8(#[from] std::str::Utf8Error),
    #[error("ungültiges XMI: {0}")]
    Invalid(String),
}

pub type XmiResult<T> = Result<T, XmiError>;

// ══ Import ═══════════════════════════════════════════════════════════════

const CONTAINMENT_PREFIX: &str = "xmi_path:";

/// Parst einen XMI-String in einen `TypedGraph` mit SOLID-Baseline.
///
/// Alle Knoten und Kanten bekommen Status `Solid`; der resultierende
/// Graph eignet sich als Baseline für eine Cascade.
pub fn import_xmi(xml: &str) -> XmiResult<TypedGraph> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut graph = TypedGraph::new();
    let mut stack: Vec<ParentFrame> = Vec::new();
    // Pfad-Zähler für Kinder ohne xmi:id (deterministische synthetische IDs).
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

/// Kontext-Frame eines offenen Eltern-Elements auf dem Parsing-Stack.
struct ParentFrame {
    ghost_id: GhostId,
    /// Pfad-Segment im synthetischen ID-Raum (für Kinder ohne xmi:id).
    path: String,
}

/// Tuple-Form für [`read_element_attributes`]: (xmi:id, xmi:type/xsi:type,
/// übrige Attribute, idref).
type ParsedXmiAttrs = (
    Option<String>,
    Option<String>,
    BTreeMap<String, String>,
    Option<String>,
);

/// Liest `xmi:id`, `xmi:type`/`xsi:type` und die übrigen Attribute.
fn read_element_attributes(elem: &BytesStart<'_>) -> XmiResult<ParsedXmiAttrs> {
    let mut xmi_id: Option<String> = None;
    let mut xmi_type: Option<String> = None;
    let mut idref: Option<String> = None;
    let mut attrs: BTreeMap<String, String> = BTreeMap::new();

    for a in elem.attributes() {
        let a = a?;
        let key = std::str::from_utf8(a.key.as_ref())?.to_string();
        let val = a.unescape_value()?.to_string();

        match key.as_str() {
            "xmi:id" => xmi_id = Some(val),
            "xmi:type" | "xsi:type" => xmi_type = Some(val),
            "xmi:idref" => idref = Some(val),
            "href" => idref = Some(val.trim_start_matches('#').to_string()),
            k if k.starts_with("xmlns") || k.starts_with("xmi:") || k.starts_with("xsi:") => {
                // Namespace-Deklarationen und XMI-Metadaten werden ignoriert.
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

    // `xmi:XMI` ist der Envelope: keine Knoten-Erzeugung, nur Stack-Frame,
    // damit die Kinder zu Roots werden.
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

    // Cross-Ref: Kind-Element mit idref/href → nur Kante, kein Knoten.
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
            // Virtuelles Frame, damit der passende End-Tag sauber popt.
            stack.push(ParentFrame {
                ghost_id: GhostId::from_baseline("__xmi_void__"),
                path: String::new(),
            });
            path_counter.push(BTreeMap::new());
        }
        return Ok(());
    }

    // Pfad-Index: deterministische Nummerierung des aktuellen Tags im
    // Geschwister-Kontext des aktuellen Frames.
    let counters = path_counter
        .last_mut()
        .ok_or_else(|| XmiError::Invalid("Pfad-Stack leer".into()))?;
    let idx = counters.entry(tag.clone()).or_insert(0);
    let my_path_segment = format!("{}[{}]", tag, *idx);
    *idx += 1;

    let parent_path = stack
        .last()
        .map(|p| p.path.clone())
        .unwrap_or_else(|| CONTAINMENT_PREFIX.to_string());
    let my_path = format!("{parent_path}/{my_path_segment}");

    // Ghost-ID bestimmen: xmi:id bevorzugt, sonst Pfad-Hash.
    let opaque = xmi_id.clone().unwrap_or_else(|| my_path.clone());
    let ghost_id = GhostId::from_opaque(&opaque);

    let type_id = xmi_type.unwrap_or_else(|| tag.clone());

    // Knoten anlegen, falls nicht bereits existent.
    if graph.get_node(&ghost_id).is_none() {
        graph.insert_node_data(crate::graph::NodeData {
            id: ghost_id,
            type_id,
            attrs,
            status: Status::Solid,
        });
    }

    // Containment-Kante von Parent zu diesem Knoten.
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

/// Serialisiert einen `TypedGraph` zurück nach XMI.
///
/// Strategie: Knoten ohne eingehende Containment-Kante werden als Roots
/// emittiert, alle anderen rekursiv entlang der Containment-Kanten. Der
/// Schlüssel `type_id` wird als `xmi:type` emittiert; der Tag-Name ist
/// das Containment-Feature (Kanten-Typ) — für Roots ist das ein
/// synthetisches `xmi:XMI`-Envelope.
pub fn export_xmi(graph: &TypedGraph) -> XmiResult<String> {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");

    let mut writer = Writer::new(Cursor::new(&mut buf));

    let mut envelope = BytesStart::new("xmi:XMI");
    envelope.push_attribute(("xmi:version", "2.0"));
    envelope.push_attribute(("xmlns:xmi", "http://www.omg.org/XMI"));
    envelope.push_attribute(("xmlns:xsi", "http://www.w3.org/2001/XMLSchema-instance"));
    writer.write_event(Event::Start(envelope.clone()))?;

    // Roots: Knoten ohne eingehende Containment-Kanten (und matchable).
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
        // Für Roots nehmen wir den Typ als Tag (EMF-Konvention:
        // `<uml:Model ...>` statt `<packagedElement xmi:type="uml:Model">`).
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
        // Zyklus (sollte bei Containment nicht passieren) — als Cross-Ref
        // emittieren.
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
        // Knoten: XMI-Root, Model, 2 Klassen, 1 Attribut = 5 (wenn xmi:XMI
        // auch als Knoten gezählt wird).
        assert!(
            g.node_count() >= 4,
            "mindestens 4 Knoten, got {}",
            g.node_count()
        );

        // xmi:id "_c1" → Person
        let c1 = GhostId::from_opaque("_c1");
        let n = g.get_node(&c1).expect("Person existiert");
        assert_eq!(n.type_id, "uml:Class");
        assert_eq!(n.attrs.get("name").map(String::as_str), Some("Person"));
    }

    #[test]
    fn import_preserves_attributes() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let root = GhostId::from_opaque("_root");
        let n = g.get_node(&root).expect("Model existiert");
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
            "Model → Person Containment fehlt"
        );
        assert!(
            g.has_edge_between(&root, &c2, "packagedElement"),
            "Model → Address Containment fehlt"
        );
    }

    #[test]
    fn roundtrip_preserves_structure() {
        let g1 = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let xml = export_xmi(&g1).expect("export ok");
        // Nicht bit-identisch, aber re-importierbar mit gleicher Anzahl
        // matchable Knoten (IDs ändern sich durch short()-Form, aber die
        // Struktur bleibt erhalten).
        let g2 = import_xmi(&xml).expect("re-import ok");
        assert_eq!(
            g1.matchable_nodes().count(),
            g2.matchable_nodes().count(),
            "Knoten-Anzahl geändert: vorher={}, nachher={}",
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
            "B → A superClass-Referenz fehlt"
        );
    }

    #[test]
    fn empty_xmi_yields_empty_graph() {
        let empty = r#"<?xml version="1.0"?><xmi:XMI xmlns:xmi="http://www.omg.org/XMI"/>"#;
        let g = import_xmi(empty).expect("empty import ok");
        assert_eq!(g.node_count(), 0);
    }

    /// Case 11 (Sirius EdgeMapping xsi:type): der XMI-Writer schreibt
    /// `xmi:type` für jeden Knoten, was Sirius beim Lesen als Subtype-
    /// Hint nutzt. Sirius akzeptiert sowohl `xmi:type` als auch
    /// `xsi:type` — beide sind in der EMF-XMI-Spec als equivalent
    /// definiert.
    #[test]
    fn case11_export_writes_xmi_type_for_each_node() {
        let g = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let exported = export_xmi(&g).expect("export ok");
        // Der Writer schreibt `xmi:type` für jeden Knoten — nicht
        // `xsi:type`, aber semantisch equivalent in EMF.
        assert!(
            exported.contains("xmi:type=\"uml:Class\"")
                || exported.contains("xmi:type=\"uml:Model\""),
            "Export muss xmi:type-Attribut für Knoten schreiben.\nExport:\n{exported}"
        );
    }

    /// Case 11 Roundtrip: import → export → import erhält die
    /// EClass-Type-Information (uml:Class etc.). Wenn der Writer
    /// xmi:type weglassen würde, würde der zweite Import die Knoten
    /// als generischen Tag-Namen-Type lesen statt als spezifische
    /// EClass — das ist genau das Sirius-EdgeMapping-Symptom.
    #[test]
    fn case11_roundtrip_preserves_eclass_type() {
        let g1 = import_xmi(MINIMAL_UML_XMI).expect("import ok");
        let class_count_pre = g1.iter_nodes().filter(|n| n.type_id == "uml:Class").count();
        let exported = export_xmi(&g1).expect("export ok");
        let g2 = import_xmi(&exported).expect("re-import ok");
        let class_count_post = g2.iter_nodes().filter(|n| n.type_id == "uml:Class").count();
        assert_eq!(
            class_count_pre, class_count_post,
            "uml:Class-Knoten-Count muss durch Roundtrip erhalten bleiben \
             (pre={class_count_pre}, post={class_count_post})"
        );
        assert!(class_count_post >= 2, "mindestens 2 Klassen erwartet");
    }
}
