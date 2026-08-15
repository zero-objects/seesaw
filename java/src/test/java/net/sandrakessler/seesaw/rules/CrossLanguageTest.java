package net.sandrakessler.seesaw.rules;
import java.util.Comparator;
import java.util.Collections;
import java.util.ArrayList;
import net.sandrakessler.seesaw.graph.Part;
import com.fasterxml.jackson.databind.node.ObjectNode;
import com.fasterxml.jackson.databind.node.ArrayNode;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.session.Fixtures;

import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

/**
 * Äquivalenz der beiden Loader, die drei Stufen aus Spec §8.
 *
 * <p>Alle Vergleichsdateien stammen aus Rust und werden dort von einem
 * {@code #[ignore]}-Test erzeugt. Ein Fixture ohne reproduzierbaren
 * Erzeuger wäre kein Beleg, sondern ein eingefrorenes Artefakt.
 */
class CrossLanguageTest {
    private static final ObjectMapper M = new ObjectMapper();

    // ── Stufe 1: gelowerter Plan ──

    /**
     * Dieselbe Regeldatei, dieselben Erzeugungspläne.
     *
     * <p>Verglichen werden die geparsten Bäume, nicht die Bytes: Rust
     * sortiert Objektfelder alphabetisch, Jackson schreibt sie in
     * Einfügereihenfolge. Der Baumvergleich ist strenger, weil er von
     * der Schreibweise unabhängig ist.
     */
    @Test
    void stufe1_gelowerterPlanIstGleich() throws Exception {
        String rules = Fixtures.resource("/fixtures/uml_java_min.json").toString();
        Graph g = new Graph();
        List<Rule> lowered = Rules.load(rules, g);

        JsonNode ausJava = M.readTree(Export.plansToJson(lowered, g));
        JsonNode ausRust = Fixtures.resource("/fixtures/uml_java_min.plans.json");

        assertEquals(ausRust, ausJava,
                "Java und Rust senken dieselbe Regeldatei verschieden");
    }

    // ── Stufe 2: Prädikat-Auswertung ──

    /**
     * Dieselbe Tabelle aus Prädikat, Wert und Erwartung, von beiden
     * Sprachen gefahren. Ein Prädikat, das hier verschieden
     * entscheidet, fällt nur an dieser Stelle auf, weil die Tabelle
     * der einzige Ort ist, an dem beide Auswertungen auf identische
     * Eingaben treffen.
     */
    @Test
    void stufe2_praedikateEntscheidenGleich() throws Exception {
        JsonNode table = Fixtures.resource("/fixtures/predicate_table.json");
        int n = 0;
        for (JsonNode c : table.get("accepted")) {
            JsonNode decl = c.get("predicate");
            Predicate p = Predicate.read(decl, "table", "n");
            JsonNode v = c.get("value");
            String value = v.isNull() ? null : v.asText();
            assertEquals(c.get("expected").asBoolean(), p.matches(value),
                    decl + " gegen " + v);
            n++;
        }
        assertEquals(42, n, "die Tabelle hat 42 Faelle");

        int r = 0;
        for (JsonNode pat : table.get("rejected_patterns")) {
            String s = pat.asText();
            assertThrows(LoadException.class,
                    () -> Predicate.parseRegex(s, "table", "n"),
                    s + " muss abgelehnt werden");
            r++;
        }
        assertEquals(24, r, "die Tabelle hat 24 abzulehnende Muster");
    }

    // ── Stufe 3: Kaskade ──

    /**
     * Identischer Eingangsgraph, identische operationelle Folge und
     * identischer Endzustand. Verglichen werden Regel, Rang, Referenzen
     * und erzeugte Knoten/Kanten jedes Kaskadeneintrags, anschließend
     * der Zustand selbst statt nur eines Hashs.
     */
    @Test
    void stufe3_kaskadeEndetGleich() throws Exception {
        String rules = Fixtures.resource("/fixtures/uml_java_min.json").toString();
        Graph g = new Graph();
        List<Rule> lowered = Rules.load(rules, g);

        Id model = g.addBaseline("m", "Model");
        Id cls = g.addBaseline("m/Person", "Class");
        Id cname = g.addBaseline("m/Person/name", "name");
        g.connect(model, cls, St.SOLID);
        g.connect(cls, cname, St.SOLID);
        Map<Id, String> vals = new HashMap<>();
        vals.put(cname, "Person");

        Engine engine = new Engine(lowered);
        engine.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        engine.run(g, vals, 1000);

        JsonNode erwartet = Fixtures.resource("/fixtures/uml_java_min.cascade.json");
        assertEquals(erwartet, cascadeState(g, vals, engine, lowered),
                "Kaskadenfolge oder Endzustand weicht von Rust ab");
    }

    /**
     * Derselbe kanonische Endzustand wie {@code cascade_state} in
     * {@code tests/format.rs}: lebende Knoten mit Typ, Wert und
     * ausgehenden Verbindungen, nach Id sortiert.
     */
    private static JsonNode cascadeState(Graph g, Map<Id, String> vals,
            Engine engine, List<Rule> rules) {
        List<ObjectNode> nodes = new ArrayList<>();
        for (Graph.Slot s : g.map.values()) {
            if (s.node == null || !s.node.status.matchable()) {
                continue;
            }
            Id id = s.node.id;
            ObjectNode o = M.createObjectNode();
            o.put("id", hex(id));
            o.put("typ", g.typeName(s.node.typ));
            String v = g.resolveValue(id, vals);
            if (v == null) {
                o.putNull("value");
            } else {
                o.put("value", v);
            }
            List<String> outs = new ArrayList<>();
            for (Part p : g.parts(id)) {
                if (p.outgoing) {
                    outs.add(hex(p.other));
                }
            }
            Collections.sort(outs);
            ArrayNode a = o.putArray("out");
            for (String x : outs) {
                a.add(x);
            }
            nodes.add(o);
        }
        nodes.sort(Comparator.comparing(o -> o.get("id").asText()));
        ObjectNode root = M.createObjectNode();
        root.put("alive", nodes.size());
        ArrayNode arr = root.putArray("nodes");
        for (ObjectNode o : nodes) {
            arr.add(o);
        }
        ArrayNode cascade = root.putArray("cascade");
        for (Engine.Entry entry : engine.cascade) {
            ObjectNode e = cascade.addObject();
            e.put("rule", rules.get(entry.ruleIx).name);
            e.put("rank", Math.toIntExact(entry.rank));
            ArrayNode refs = e.putArray("refs");
            for (Id id : entry.refs) refs.add(hex(id));
            ArrayNode created = e.putArray("created");
            for (Id id : entry.created) created.add(hex(id));
            ArrayNode edges = e.putArray("created_edges");
            for (Id id : entry.createdEdges) edges.add(hex(id));
        }
        return root;
    }

    private static String hex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) {
            sb.append(String.format("%02x", b & 0xFF));
        }
        return sb.toString();
    }
}
