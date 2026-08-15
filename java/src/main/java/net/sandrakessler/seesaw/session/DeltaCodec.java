package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.ident.Ident;

import java.util.ArrayDeque;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.Transform;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;


/**
 * Δ-JSON → Graph (E2). Parst das {@code submitDelta}-Format des
 * Piloten ({@code {"origin": …, "op_star": [AddNode|AddEdge|DelNode|
 * DelEdge|SetAttr]}}, Erzeuger: {@code seesaw.emf.DeltaBuilder} bzw.
 * {@code seesaw.jdt.AstDiffBuilder}) und wendet die Ops auf einen
 * {@link Graph} an. Spiegel der Op-Semantik der JNI-Schicht
 * ({@code seesaw-jni::apply_json_op}), übersetzt in das Modell:
 *
 * <ul>
 *   <li>Knoten-Attribute sind Blatt-Subknoten (Typ = Attributname),
 *       Werte wohnen in der Value-Map — {@code attrs}/{@code SetAttr}
 *       werden zu Blättern bzw. Wert-Updates.</li>
 *   <li>Getypte Kanten sind anonyme Verbindungen (Direct) bzw.
 *       Rollen-Knoten (Reified) gemäß der Prüfpunkt-2-Tabelle des
 *       Regelsatzes ({@link SessionRules}).</li>
 *   <li>Externe Opaque-Ids lösen über {@code identBaseline} auf; eine
 *       64-Hex-Opaque, die einen existierenden Knoten trifft, löst wie
 *       in der JNI-Schicht direkt auf diesen auf (rc8-Forward-Delete-
 *       Adressierung); {@link #registerOpaque} ist die
 *       registerExternalOpaque-Bridge (rc7).</li>
 * </ul>
 *
 * <p>Der Codec mutiert NUR den Graphen/die Value-Map; Match-Buchhaltung
 * (seedRouted/elementsAdded/retractFor/linkRemoved) ist Sache des
 * Aufrufers und wird über das {@link Result} gefüttert.
 *
 * <p>SetAttr auf einem ABGELEITETEN Blatt (regel-erzeugt, z. B. Name
 * einer materialisierten JavaClass) schreibt durch die Transform-Kette
 * auf das Quell-Blatt zurück (inverse Transformationen, geschlossene
 * Menge) — das ist die hiesige Form der alten „gebundenes Attr als SetAttr
 * propagiert"-Semantik (rc7 A8). Konstanten-Blätter sind nicht
 * schreibbar (Wert wohnt im Regelsatz) → Op-Fehler.
 */
public final class DeltaCodec {

    private static final ObjectMapper M = new ObjectMapper();

    /** Nicht final seit E3: fold() ersetzt den Graphen durch die
     *  Materialisierung ({@link #rebindGraph}). */
    private Graph g;
    private final Map<Id, String> values;
    private final SessionRules meta;

    private final Map<String, Id> opaqueToId = new HashMap<>();
    private final Map<Id, String> idToOpaque = new HashMap<>();
    /** Zur Laufzeit beobachtete Attr-Schlüssel (ergänzt attr_types). */
    private final Set<String> observedAttrTypes = new HashSet<>();

    public DeltaCodec(Graph g, Map<Id, String> values, SessionRules meta) {
        this.g = g;
        this.values = values;
        this.meta = meta;
    }

    /** Ergebnis eines Δ-Applies — Futter für die Engine-Buchhaltung. */
    public static final class Result {
        String origin = "unknown";
        int received;
        int applied;
        final List<String> errors = new ArrayList<>();
        /** Neu angelegte Knoten (inkl. Blätter/Rollen) → elementsAdded. */
        final List<Id> newNodes = new ArrayList<>();
        /** Tombstonete Knoten → retractFor + elementRemoved. */
        final List<Id> removedNodes = new ArrayList<>();
        /** Tombstonete Verbindungen (Endpunkt-Paare) → linkRemoved. */
        final List<Id[]> removedLinks = new ArrayList<>();
        /** Berührte Typ-Namen → seedRouted (Δ-Routing). */
        final Set<String> deltaTypes = new LinkedHashSet<>();
    }

    // ── Opaque-Auflösung (Spiegel von Session::resolve_id) ──

    /** E3-Fold: auf die materialisierte Baseline umbinden — Opaque-
     *  Zuordnungen und beobachtete Attr-Typen überleben (Ids sind
     *  über die Materialisierung stabil). */
    public void rebindGraph(Graph folded) {
        this.g = folded;
    }

    /** rc7-Bridge: externe Opaque für einen EXISTIERENDEN Knoten. */
    public void registerOpaque(String opaque, Id id) {
        opaqueToId.put(opaque, id);
        idToOpaque.put(id, opaque);
    }

    public String opaqueOf(Id id) {
        return idToOpaque.get(id);
    }

    public Id resolve(String opaque) {
        Id known = opaqueToId.get(opaque);
        if (known != null) return known;
        // 64-Hex-Opaque eines existierenden Knotens = direkte Adresse
        // (rc8: seesawId über attach/detach hinweg stabil).
        if (opaque.length() == 64 && opaque.chars().allMatch(
                c -> Character.digit(c, 16) >= 0)) {
            Id direct = Id.fromHex(opaque);
            if (g.node(direct) != null) return direct;
        }
        Id id = Ident.identBaseline(opaque);
        registerOpaque(opaque, id);
        return id;
    }

    /** Kanonische Opaque einer Id — für abgeleitete Blatt-/Rollen-Pfade. */
    private String canonicalOpaque(Id id) {
        String o = idToOpaque.get(id);
        if (o != null) return o;
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    private Id ensureNode(String opaque, String typName) {
        Id id = resolve(opaque);
        if (g.node(id) == null) {
            g.insertExternal(id, typName);
        }
        return id;
    }

    /** Attr-Blatt-Typ? (Tabelle ∪ Laufzeit-Beobachtung ∪ Ableitung). */
    public boolean isLeafNode(Node n) {
        if (n.derivedSource != null || n.konstIx >= 0) return true;
        String t = g.typeName(n.typ);
        return meta.attrTypes.contains(t) || observedAttrTypes.contains(t);
    }

    // ── Δ-Apply ──

    /** Parst und wendet ein Δ-JSON an; Fehler pro Op, kein Abbruch. */
    public Result apply(String deltaJson) {
        Result res = new Result();
        JsonNode root;
        try {
            root = M.readTree(deltaJson);
        } catch (Exception e) {
            res.errors.add("JSON parse error: " + e.getMessage());
            return res;
        }
        res.origin = root.path("origin").asText("unknown");
        JsonNode ops = root.path("op_star");
        res.received = ops.size();
        int idx = 0;
        for (Iterator<JsonNode> it = ops.elements(); it.hasNext(); idx++) {
            JsonNode op = it.next();
            try {
                applyOp(op, res);
                res.applied++;
            } catch (RuntimeException e) {
                res.errors.add("op[" + idx + "]: " + e.getMessage());
            }
        }
        return res;
    }

    private void applyOp(JsonNode op, Result res) {
        String type = op.path("type").asText("");
        switch (type) {
            case "AddNode" -> applyAddNode(op, res);
            case "AddEdge" -> applyAddEdge(op, res);
            case "DelNode" -> applyDelNode(op, res);
            case "DelEdge" -> applyDelEdge(op, res);
            case "SetAttr" -> applySetAttr(op, res);
            default -> throw new IllegalArgumentException(
                "unknown op type '" + type + "'");
        }
    }

    private static String req(JsonNode op, String field) {
        JsonNode v = op.get(field);
        if (v == null || !v.isTextual()) {
            throw new IllegalArgumentException(
                "missing or non-string '" + field + "'");
        }
        return v.asText();
    }

    private void applyAddNode(JsonNode op, Result res) {
        String parentOpaque = req(op, "parent");
        String childOpaque = op.hasNonNull("childId")
            ? req(op, "childId") : req(op, "opaqueId");
        String typeId = req(op, "typeId");
        String edgeType = op.path("edgeType").asText("contains");

        Id parent = ensureNode(parentOpaque, "Unknown");
        Id child = resolve(childOpaque);
        boolean fresh = g.node(child) == null;
        if (fresh) {
            g.insertExternal(child, typeId);
            res.newNodes.add(child);
            JsonNode attrs = op.path("attrs");
            for (Iterator<String> it = attrs.fieldNames(); it.hasNext(); ) {
                String key = it.next();
                Id leaf = addAttrLeaf(child, childOpaque, key);
                values.put(leaf, attrs.path(key).asText(""));
                res.newNodes.add(leaf);
                res.deltaTypes.add(key);
            }
        }
        connectByKind(parent, child, edgeType, res);
        res.deltaTypes.add(typeId);
    }

    private void applyAddEdge(JsonNode op, Result res) {
        String source = req(op, "source");
        String target = req(op, "target");
        String edgeType = req(op, "edgeType");
        Id s = ensureNode(source, "Unknown");
        Id t = ensureNode(target, "Unknown");
        connectByKind(s, t, edgeType, res);
        addTypeOf(s, res);
        addTypeOf(t, res);
    }

    /**
     * DelNode mit korrespondenz-folgender Retraction (E4). Spiegel von
     * der Retraktions-Kaskade der ersten Generation:
     * <ol>
     *   <li>jede inzidente Verbindung des gelöschten Knotens fällt mit
     *       (dort: induzierte DelEdge-Ops);</li>
     *   <li>Corr-Hop: über jeden benachbarten Corr-Knoten zum Partner —
     *       Corr UND Partner werden mit-tombstonet; Transitivität über
     *       die Worklist (dort: Re-Expansion je induziertem Op).</li>
     * </ol>
     * Die Rust-Seite hat dieselbe Lücke (Retraction ist provenienz-gebunden
     * und GHOST-gated) — deshalb lebt der Corr-Hop HIER im Codec über
     * die Wire-Struktur (Corr-Typen aus {@link SessionRules}), nicht
     * als Engine-Eingriff. Er greift nur für SOLID-Corrs
     * (materialisiert, post-fold): Ghost-Erzeugnisse retraktiert die
     * Engine über die Match-Provenienz, inklusive Resurrektions-
     * Fenster (TT), das ein harter Tombstone zerstören würde.
     *
     * <p>E3-Erbe: Attr-Blätter jedes tombstoneten Knotens fallen mit
     * (Paritaet zur ersten Generation — Attrs wohnen dort IM Knoten).
     */
    private void applyDelNode(JsonNode op, Result res) {
        String target = req(op, "target");
        Id id = resolve(target);
        if (g.node(id) == null) {
            throw new IllegalStateException("DelNode target not found: " + target);
        }
        java.util.ArrayDeque<Id> work = new java.util.ArrayDeque<>();
        Set<Id> seen = new HashSet<>();
        work.add(id);
        while (!work.isEmpty()) {
            Id cur = work.poll();
            if (!seen.add(cur)) continue;
            Node n = g.node(cur);
            if (n == null || n.status == St.TOMBSTONE) continue;
            addTypeOf(cur, res);
            tombstoneWithLeaves(cur, res);
            for (Part p : g.parts(cur)) {
                Node other = g.node(p.other);
                if (other == null) continue;
                // 1. Inzidente Verbindung fällt mit.
                Conn c = g.conn(p.connection);
                if (c != null && c.status != St.TOMBSTONE) {
                    g.setConnectionStatus(p.connection, St.TOMBSTONE);
                    if (!isLeafNode(other)) {
                        res.removedLinks.add(new Id[] { c.source, c.target });
                    }
                }
                // 2. Corr-Hop (nur SOLID — Ghost geht über Provenienz).
                if (other.status == St.SOLID
                        && meta.corrTypes.contains(g.typeName(other.typ))
                        && !seen.contains(p.other)) {
                    for (Part q : g.parts(p.other)) {
                        if (!q.other.equals(cur)) work.add(q.other);
                    }
                    work.add(p.other);
                }
            }
        }
    }

    /** Knoten + seine Attr-Blätter (samt Trage-Verbindung) tombstonen;
     *  alles Tombstonete geht in removedNodes (Match-Entkräftung). */
    private void tombstoneWithLeaves(Id id, Result res) {
        g.setNodeStatus(id, St.TOMBSTONE);
        res.removedNodes.add(id);
        for (Part p : g.parts(id)) {
            if (!p.outgoing) continue;
            Node leaf = g.node(p.other);
            if (leaf == null || !isLeafNode(leaf)
                    || leaf.status == St.TOMBSTONE) continue;
            g.setNodeStatus(p.other, St.TOMBSTONE);
            Conn c = g.conn(p.connection);
            if (c != null) g.setConnectionStatus(p.connection, St.TOMBSTONE);
            res.removedNodes.add(p.other);
        }
    }

    private void applyDelEdge(JsonNode op, Result res) {
        String source = req(op, "source");
        String target = req(op, "target");
        String edgeType = req(op, "edgeType");
        Id s = resolve(source);
        Id t = resolve(target);
        addTypeOf(s, res);
        addTypeOf(t, res);
        if (meta.reifiedKinds.contains(edgeType)) {
            // Rollen-Knoten samt beider Verbindungen tombstonen.
            Id role = Ident.identBaseline(roleOpaque(s, edgeType, t));
            if (g.node(role) == null) {
                throw new IllegalStateException("DelEdge target not found: "
                    + source + "→" + target + " (" + edgeType + ")");
            }
            tombstoneConn(s, role);
            tombstoneConn(role, t);
            g.setNodeStatus(role, St.TOMBSTONE);
            res.removedNodes.add(role);
            res.removedLinks.add(new Id[] { s, role });
            res.removedLinks.add(new Id[] { role, t });
            return;
        }
        Conn c = g.conn(Ident.identConnection(s, t));
        if (c == null) {
            throw new IllegalStateException("DelEdge target not found: "
                + source + "→" + target + " (" + edgeType + ")");
        }
        g.setConnectionStatus(c.id, St.TOMBSTONE);
        res.removedLinks.add(new Id[] { s, t });
    }

    private void applySetAttr(JsonNode op, Result res) {
        String target = req(op, "target");
        String key = req(op, "key");
        String value = op.path("value").asText("");

        Id id = ensureNode(target, "Unknown");
        Id leaf = g.childLeafOfType(id, key);
        if (leaf == null) {
            leaf = addAttrLeaf(id, canonicalOpaque(id), key);
            values.put(leaf, value);
            res.newNodes.add(leaf);
        } else {
            writeThrough(leaf, value);
        }
        addTypeOf(id, res);
        res.deltaTypes.add(key);
        // rc8 J8: Rename ändert die name-abgeleitete Opaque — neue
        // Opaque auf DENSELBEN Knoten realiasen (kein Phantom).
        JsonNode realias = op.get("realiasTo");
        if (realias != null && realias.isTextual()) {
            registerOpaque(realias.asText(), id);
        }
    }

    /**
     * Wert-Update mit Durchschrieb: Baseline-Blatt direkt; abgeleitetes
     * Blatt entlang der Transform-Kette invers bis zum Quell-Blatt
     * (rc7-A8-Parität). Konstanten-Blatt → Fehler.
     */
    private void writeThrough(Id leaf, String value) {
        List<Chain> chain = new ArrayList<>();
        Id cur = leaf;
        while (true) {
            Node n = g.node(cur);
            if (n == null) {
                throw new IllegalStateException("SetAttr: Blatt-Kette gerissen");
            }
            if (n.konstIx >= 0) {
                throw new IllegalStateException(
                    "SetAttr auf Konstanten-Blatt (Wert wohnt im Regelsatz)");
            }
            if (n.derivedSource == null) break;
            chain.add(n.derivedTransform);
            cur = n.derivedSource;
        }
        String v = value;
        for (Chain t : chain) {
            String quelle = t.invertChecked(v);
            if (quelle == null) {
                // Spiegel von Chain::invert_checked: entweder ist die
                // Rueckwaerts-Kette nicht anwendbar, oder die
                // zurueckgerechnete Quelle ergibt vorwaerts einen
                // ANDEREN Wert — dann ist der geforderte Zielwert
                // ueber diese Kette gar nicht erreichbar. Beides
                // stillschweigend zu schreiben hiesse, das Blatt
                // danach auf etwas anderes aufloesen zu lassen als
                // verlangt.
                throw new IllegalStateException(
                    "SetAttr: kein konsistenter Quellwert fuer \"" + v
                        + "\" ueber die Transform-Kette " + t);
            }
            v = quelle;
        }
        values.put(cur, v);
    }

    // ── Helfer ──

    private Id addAttrLeaf(Id owner, String ownerOpaque, String key) {
        Id leaf = resolve(ownerOpaque + "/" + key);
        if (g.node(leaf) == null) {
            g.insertExternal(leaf, key);
        }
        g.connect(owner, leaf, St.SOLID);
        observedAttrTypes.add(key);
        return leaf;
    }

    /** Direct: anonyme Verbindung; Reified: Rollen-Knoten einschieben
     *  (Schema des Konverters der ersten Generation). */
    private void connectByKind(Id s, Id t, String kind, Result res) {
        if (meta.reifiedKinds.contains(kind)) {
            Id role = resolve(roleOpaque(s, kind, t));
            if (g.node(role) == null) {
                g.insertExternal(role, kind);
                res.newNodes.add(role);
            }
            g.connect(s, role, St.SOLID);
            g.connect(role, t, St.SOLID);
            res.deltaTypes.add(kind);
            return;
        }
        g.connect(s, t, St.SOLID);
    }

    private String roleOpaque(Id s, String kind, Id t) {
        return canonicalOpaque(s) + "/" + kind + "/" + canonicalOpaque(t);
    }

    private void tombstoneConn(Id s, Id t) {
        Conn c = g.conn(Ident.identConnection(s, t));
        if (c != null) g.setConnectionStatus(c.id, St.TOMBSTONE);
    }

    private void addTypeOf(Id id, Result res) {
        Node n = g.node(id);
        if (n != null) res.deltaTypes.add(g.typeName(n.typ));
    }
}
