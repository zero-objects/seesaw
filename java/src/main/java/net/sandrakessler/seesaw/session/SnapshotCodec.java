package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.rules.Transform;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.util.HashMap;
import java.util.Map;


/**
 * Graphen → Snapshot-JSON (E2). Erzeugt die Struktur, die die
 * bestehenden Konsumenten der JNI-Session lesen —
 * {@code EmfModelWriter}, {@code JdtAstWriter}/
 * {@code SnapshotToJavaAstConverter}, {@code SnapshotToAdjacencyConverter},
 * {@code WellFormednessGate}:
 *
 * <pre>
 * {"nodeCount": n, "edgeCount": m,
 *  "nodes": [{"id": 8-Hex, "idFull": 64-Hex, "opaque": …|null,
 *             "type": …, "status": Solid|Ghost|TentativeTombstone|Tombstone,
 *             "attrs": {…}}],
 *  "edges": [{"source": 8-Hex, "target": 8-Hex, "type": …, "status": …}]}
 * </pre>
 *
 * <p>Rückübersetzung des Modells in die alte Wire-Fassung:
 * <ul>
 *   <li>Attr-Blätter (Typ = Attributname, abgeleitete Blätter,
 *       Konstanten-Blätter) erscheinen NICHT als Knoten, sondern als
 *       {@code attrs}-Einträge ihres Eigners (Wert über den
 *       Resolver, d. h. Transform-Ketten aufgelöst).</li>
 *   <li>Anonyme Verbindungen bekommen ihren alten Kantentyp aus der
 *       Endpunkt-Typ-Tabelle des Regelsatzes; Verbindungen von/zu
 *       Corr-Knoten heißen {@code corrL}/{@code corrR} (Anker→Corr =
 *       corrL, Corr→Erzeugtes = corrR — richtungs-relativ wie dort);
 *       Rollen-Knoten (Reified) kollabieren zu einer getypten Kante.</li>
 * </ul>
 */
public final class SnapshotCodec {

    private static final ObjectMapper M = new ObjectMapper();

    private SnapshotCodec() {}

    public static String hex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    /** 8-Hex-Kurzform — Spiegel von GhostId::short (JNI-Snapshot-id). */
    public static String shortHex(Id id) {
        return hex(id).substring(0, 8);
    }

    public static String wireStatus(St st) {
        return switch (st) {
            case SOLID -> "Solid";
            case GHOST -> "Ghost";
            case TENTATIVE_TOMBSTONE -> "TentativeTombstone";
            case TOMBSTONE -> "Tombstone";
        };
    }

    /**
     * Rendert den Snapshot. {@code codec} liefert Blatt-Klassifikation
     * und Opaque-Rückauflösung der Session.
     */
    public static String render(Graph g, Map<Id, String> values,
            SessionRules meta, DeltaCodec codec) {
        ObjectNode root = M.createObjectNode();
        ArrayNode nodes = M.createArrayNode();
        ArrayNode edges = M.createArrayNode();

        // Klassifikation einmal vorab: Blatt / Rolle / Fläche.
        Map<Id, Node> surface = new HashMap<>();
        Map<Id, Node> roles = new HashMap<>();
        for (Node n : g.allNodes()) {
            if (codec.isLeafNode(n)) continue;
            if (meta.reifiedKinds.contains(g.typeName(n.typ))) {
                roles.put(n.id, n);
            } else {
                surface.put(n.id, n);
            }
        }

        for (Node n : surface.values()) {
            ObjectNode jn = nodes.addObject();
            jn.put("id", shortHex(n.id));
            jn.put("idFull", hex(n.id));
            String opaque = codec.opaqueOf(n.id);
            if (opaque != null) jn.put("opaque", opaque);
            else jn.putNull("opaque");
            jn.put("type", g.typeName(n.typ));
            jn.put("status", wireStatus(n.status));
            ObjectNode attrs = jn.putObject("attrs");
            // Blatt-Status bewusst NICHT gefiltert: Knoten der alten Fassung tragen
            // ihre attrs auch als Tombstone (das WellFormednessGate
            // liest sie statusblind) — ein mit-tombstonetes Blatt
            // (E3-DelNode) bleibt auf dem Wire sichtbar, bis die
            // Materialisierung den ganzen Knoten wegfallen lässt.
            for (Part p : g.parts(n.id)) {
                if (!p.outgoing) continue;
                Node other = g.node(p.other);
                if (other == null || !codec.isLeafNode(other)) continue;
                String key = g.typeName(other.typ);
                if (attrs.has(key)) continue;
                String v = g.resolveValue(p.other, values);
                if (v != null) attrs.put(key, v);
            }
        }

        for (Conn c : g.allConnections()) {
            Node s = surface.get(c.source);
            Node t = surface.get(c.target);
            if (s != null && t != null) {
                addEdge(edges, c.source, c.target, c.status,
                    edgeType(g, meta, s, t));
                continue;
            }
            // Rollen-Knoten: eingehende Verbindung überspringen, die
            // ausgehende zur getypten alten Kante Eigner→Ziel kollabieren.
            Node role = roles.get(c.source);
            if (role != null && t != null) {
                Id owner = null;
                for (Part p : g.parts(role.id)) {
                    if (!p.outgoing && surface.containsKey(p.other)) {
                        owner = p.other;
                        break;
                    }
                }
                if (owner != null) {
                    addEdge(edges, owner, c.target, c.status,
                        g.typeName(role.typ));
                }
            }
            // Blatt-Verbindungen: als attrs abgebildet, keine Kanten.
        }

        root.put("nodeCount", nodes.size());
        root.put("edgeCount", edges.size());
        root.set("nodes", nodes);
        root.set("edges", edges);
        return root.toString();
    }

    private static void addEdge(ArrayNode edges,
            Id source, Id target, St status, String type) {
        ObjectNode je = edges.addObject();
        je.put("source", shortHex(source));
        je.put("target", shortHex(target));
        je.put("type", type);
        je.put("status", wireStatus(status));
    }

    /** alten Kantentyp einer anonymen Verbindung: corrL/corrR an
     *  Corr-Knoten, sonst Endpunkt-Typ-Tabelle, sonst leer. */
    private static String edgeType(Graph g, SessionRules meta,
            Node s, Node t) {
        String sTyp = g.typeName(s.typ);
        String tTyp = g.typeName(t.typ);
        if (meta.corrTypes.contains(tTyp)) return "corrL";
        if (meta.corrTypes.contains(sTyp)) return "corrR";
        String kind = meta.edgeKindByCombo.get(SessionRules.comboKey(sTyp, tTyp));
        return kind != null ? kind : "";
    }
}
