package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.junit.jupiter.api.Test;


/**
 * E2 Schritt 1: Graphen → Snapshot-JSON. Struktur-Assertions sind aus
 * den Konsumenten abgeleitet (EmfModelWriter: nodes[].id/idFull/type/
 * status/attrs; SnapshotToJavaAstConverter + WellFormednessGate:
 * edges[].source/target/type/status, Kanten-Endpunkte existieren als
 * Knoten-Ids). Kein Rust-Golden greifbar (JNI-Session schreibt keine
 * Snapshot-Fixtures) → Roundtrip- und Konsumenten-Struktur-Tests.
 */
class SnapshotCodecTest {

    private static final ObjectMapper M = new ObjectMapper();

    private static SessionRules demoMeta() {
        return new SessionRules(
            List.of(),
            Set.of("name", "type"),
            Set.of("CorrClass"),
            Map.of(
                SessionRules.comboKey("Model", "Class"), "classes",
                SessionRules.comboKey("Model", "JavaClass"), "javaClasses"),
            Set.of());
    }

    private static String render(Graph g, Map<Id, String> vals,
            DeltaCodec codec) {
        return SnapshotCodec.render(g, vals, demoMeta(), codec);
    }

    @Test
    void blattWirdZuAttrsEintragNichtZuKnoten() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}}
            ]}""");

        JsonNode root = M.readTree(render(g, vals, codec));

        Set<String> types = new HashSet<>();
        JsonNode classNode = null;
        for (JsonNode n : root.path("nodes")) {
            types.add(n.path("type").asText());
            if ("Class".equals(n.path("type").asText())) classNode = n;
        }
        assertTrue(types.contains("Model"));
        assertTrue(types.contains("Class"));
        assertTrue(!types.contains("name"), "Blatt ist kein Snapshot-Knoten");
        assertNotNull(classNode);
        assertEquals("Foo", classNode.path("attrs").path("name").asText());
        assertEquals("Solid", classNode.path("status").asText());
        assertEquals(64, classNode.path("idFull").asText().length());
        assertEquals(8, classNode.path("id").asText().length());
        assertEquals("c1", classNode.path("opaque").asText());
        assertEquals(root.path("nodes").size(), root.path("nodeCount").asInt());
        assertEquals(root.path("edges").size(), root.path("edgeCount").asInt());
    }

    @Test
    void kantenTypAusEndpunktTabelleUndKeineDanglingEdges() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}}
            ]}""");

        JsonNode root = M.readTree(render(g, vals, codec));

        Set<String> nodeIds = new HashSet<>();
        for (JsonNode n : root.path("nodes")) nodeIds.add(n.path("id").asText());
        boolean sawClasses = false;
        for (JsonNode e : root.path("edges")) {
            assertTrue(nodeIds.contains(e.path("source").asText()),
                "Quell-Knoten existiert (WellFormednessGate DANGLING_EDGE)");
            assertTrue(nodeIds.contains(e.path("target").asText()),
                "Ziel-Knoten existiert");
            if ("classes".equals(e.path("type").asText())) sawClasses = true;
        }
        assertTrue(sawClasses, "Model→Class trägt den alten Kantentyp 'classes'");
    }

    @Test
    void corrVerbindungenHeissenCorrLUndCorrR() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}}
            ]}""");
        // Regel-Erzeugnis von Hand: Class → CorrClass → JavaClass
        // (Provenienz-Kette Anker → Corr → Erzeugtes, Spec §1.3).
        Id c = Ident.identBaseline("c1");
        Id corr = g.addGhost(c, "CorrClass");
        Id jc = g.addGhost(corr, "JavaClass");
        g.connect(c, corr, St.GHOST);
        g.connect(corr, jc, St.GHOST);

        JsonNode root = M.readTree(render(g, vals, codec));

        String corrL = null, corrR = null;
        for (JsonNode e : root.path("edges")) {
            if ("corrL".equals(e.path("type").asText()))
                corrL = e.path("target").asText();
            if ("corrR".equals(e.path("type").asText()))
                corrR = e.path("source").asText();
        }
        assertNotNull(corrL, "Anker→Corr = corrL");
        assertNotNull(corrR, "Corr→Erzeugtes = corrR");
        assertEquals(corrL, corrR, "beide enden am selben Corr-Knoten");
    }

    @Test
    void roundtripParseSerializeParseIstStabil() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}},
              {"type":"SetAttr","target":"c1","key":"name","value":"Bar"}
            ]}""");

        String s1 = render(g, vals, codec);
        String s2 = render(g, vals, codec);
        assertEquals(M.readTree(s1), M.readTree(s2),
            "Snapshot ohne Mutation stabil");

        // Δ aus dem Snapshot abgeleitet in eine ZWEITE Session spielen:
        // gleiche Fläche (Typen, Attrs) — Parse→Apply→Serialize stabil.
        Graph g2 = new Graph();
        Map<Id, String> vals2 = new HashMap<>();
        DeltaCodec codec2 = new DeltaCodec(g2, vals2, demoMeta());
        codec2.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Bar"}}
            ]}""");
        assertEquals(M.readTree(s1), M.readTree(render(g2, vals2, codec2)),
            "identische Fläche ⇒ identischer Snapshot (strukturelle Identität)");
    }

    @Test
    void tombstoneStatusErscheintAufDemWire() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}},
              {"type":"DelNode","target":"c1"}
            ]}""");

        JsonNode root = M.readTree(render(g, vals, codec));
        String status = null;
        for (JsonNode n : root.path("nodes")) {
            if ("Class".equals(n.path("type").asText()))
                status = n.path("status").asText();
        }
        assertEquals("Tombstone", status);
    }
}
