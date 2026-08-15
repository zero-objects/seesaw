package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;

import org.junit.jupiter.api.Test;

/**
 * Etappe 8: ROUNDTRIP-Äquivalenz des Java-Ports gegen die
 * Rust-Referenz — nicht nur Forward. Prüft seed_routed, elements_added,
 * link_removed und run/Termination bit-exakt (Schrittzahlen, Verdikt,
 * Endzustands-Fingerprint) gegen den aus der Rust-Engine exportierten
 * Golden ({@code /fixtures/roundtrip_golden.json}, Regeln {@code /fixtures/rules_f2p.json}).
 * Der Fingerprint (FNV-1a-64 über die kanonisierte lebendige Knoten-/
 * Kanten-Topologie) ist auf beiden Seiten identisch konstruiert.
 */
class RoundtripEquivalenceTest {

    // ── Seed-Bau, identisch zu tests/v2_java_golden_export.rs ──

    private static void seedFwd(int n, Graph g, Map<Id, String> vals) {
        for (int i = 0; i < n; i++) {
            Id f = g.addBaseline("f" + i, "Family");
            Id r = g.addBaseline("f" + i + "/father", "Father");
            Id m = g.addBaseline("f" + i + "/father/m", "Member");
            Id leaf = g.addBaseline("f" + i + "/father/m/fn", "firstName");
            g.connect(f, r, St.SOLID);
            g.connect(r, m, St.SOLID);
            g.connect(m, leaf, St.SOLID);
            vals.put(leaf, "John" + i);
        }
    }

    private static void seedBwd(int n, Graph g, Map<Id, String> vals) {
        for (int i = 0; i < n; i++) {
            Id male = g.addBaseline("p" + i, "Male");
            Id leaf = g.addBaseline("p" + i + "/name", "name");
            g.connect(male, leaf, St.SOLID);
            vals.put(leaf, "John" + i);
        }
    }

    private static List<Id> addFamily(Graph g, Map<Id, String> vals, int i) {
        Id f = g.addBaseline("f" + i, "Family");
        Id r = g.addBaseline("f" + i + "/father", "Father");
        Id m = g.addBaseline("f" + i + "/father/m", "Member");
        Id leaf = g.addBaseline("f" + i + "/father/m/fn", "firstName");
        g.connect(f, r, St.SOLID);
        g.connect(r, m, St.SOLID);
        g.connect(m, leaf, St.SOLID);
        vals.put(leaf, "John" + i);
        return List.of(f, r, m, leaf);
    }

    private static int tombstoned(Graph g) {
        int c = 0;
        for (Graph.Slot s : g.map.values())
            if (s.node != null && s.node.status == St.TOMBSTONE) c++;
        return c;
    }

    private static Engine.Termination termOf(String rustDebug) {
        switch (rustDebug) {
            case "Duplication": return Engine.Termination.DUPLICATION;
            case "Convergence": return Engine.Termination.CONVERGENCE;
            case "StepLimit": return Engine.Termination.STEP_LIMIT;
            case "Contradiction": return Engine.Termination.CONTRADICTION;
            default: throw new IllegalArgumentException(rustDebug);
        }
    }

    private JsonNode golden(String scenario) throws Exception {
        return Fixtures.resource("/fixtures/roundtrip_golden.json").get(scenario);
    }

    private static void assertEndState(JsonNode exp, Graph g) {
        Fingerprint.Result fp = Fingerprint.of(g);
        assertEquals(exp.get("alive_nodes").asInt(), fp.aliveNodes, "alive_nodes");
        assertEquals(exp.get("fingerprint").asText(), fp.hex, "fingerprint");
    }

    private static void saturate(Engine e, Graph g, Map<Id, String> vals) {
        while (e.step(g, vals) != null) { /* weiter */ }
    }

    // ── Szenarien ──

    @Test
    void forwardRun() throws Exception {
        JsonNode exp = golden("forward_run");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(3, g, vals);
        List<Rule> rules = Fixtures.rules("f2p", g);
        Engine e = new Engine(rules);
        Engine.Termination term = e.run(g, vals, 10_000);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEquals(termOf(exp.get("termination").asText()), term, "termination");
        assertEndState(exp, g);
    }

    @Test
    void routedForward() throws Exception {
        JsonNode exp = golden("routed_forward");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(2, g, vals);
        List<Rule> rules = Fixtures.rules("f2p", g);
        List<String> delta = rules.get(0).inputTypes; // fwd
        assertEquals(jsonList(exp.get("delta_types")), new TreeSet<>(delta), "delta_types");
        Engine e = new Engine(rules);
        e.seedRouted(g, vals, delta);
        saturate(e, g, vals);
        e.consolidate(g);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEndState(exp, g);
    }

    @Test
    void routedBackward() throws Exception {
        JsonNode exp = golden("routed_backward");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedBwd(2, g, vals);
        List<Rule> rules = Fixtures.rules("f2p", g);
        List<String> delta = rules.get(1).inputTypes; // bwd
        assertEquals(jsonList(exp.get("delta_types")), new TreeSet<>(delta), "delta_types");
        Engine e = new Engine(rules);
        e.seedRouted(g, vals, delta);
        saturate(e, g, vals);
        e.consolidate(g);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEndState(exp, g);
    }

    @Test
    void incrementalAdd() throws Exception {
        JsonNode exp = golden("incremental_add");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(2, g, vals);
        List<Rule> all = Fixtures.rules("f2p", g);
        Engine e = new Engine(List.of(all.get(0))); // nur fwd
        e.seed(g, vals);
        saturate(e, g, vals);
        assertEquals(exp.get("cascade_before").asInt(), e.cascadeLen, "cascade_before");
        List<Id> newNodes = addFamily(g, vals, 2);
        e.elementsAdded(g, vals, newNodes);
        saturate(e, g, vals);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEndState(exp, g);
    }

    @Test
    void linkRetract() throws Exception {
        JsonNode exp = golden("link_retract");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(2, g, vals);
        List<Rule> all = Fixtures.rules("f2p", g);
        Engine e = new Engine(List.of(all.get(0)));
        e.run(g, vals, 10_000);
        Id[] refs0 = e.cascade.get(0).refs;
        e.linkRemoved(g, refs0[2], refs0[3]);
        e.consolidate(g);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEquals(exp.get("tombstoned").asInt(), tombstoned(g), "tombstoned");
        assertEndState(exp, g);
    }

    @Test
    void resurrection() throws Exception {
        JsonNode exp = golden("resurrection");
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(1, g, vals);
        List<Rule> all = Fixtures.rules("f2p", g);
        Engine e = new Engine(List.of(all.get(0)));
        e.run(g, vals, 10_000);
        Id[] refs0 = e.cascade.get(0).refs;
        e.linkRemoved(g, refs0[2], refs0[3]);              // Erzeugtes → TT
        e.elementsAdded(g, vals, List.of(refs0[2]));       // Re-Anker → Reklamation
        saturate(e, g, vals);
        e.consolidate(g);
        assertEquals(exp.get("cascade").asInt(), e.cascadeLen, "cascade");
        assertEquals(exp.get("tombstoned").asInt(), tombstoned(g), "tombstoned");
        assertEndState(exp, g);
    }

    private static TreeSet<String> jsonList(JsonNode arr) {
        TreeSet<String> out = new TreeSet<>();
        for (JsonNode n : arr) out.add(n.asText());
        return out;
    }
}
