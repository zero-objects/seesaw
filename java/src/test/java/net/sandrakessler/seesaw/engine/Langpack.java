package net.sandrakessler.seesaw.engine;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import net.sandrakessler.seesaw.bench.Counters;
import net.sandrakessler.seesaw.bench.Section;
import net.sandrakessler.seesaw.bench.Seeds;
import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.hash.Blake3;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Workload „Sprachpack“ der Rust-Suite ({@code benches/suite/langpack.rs}),
 * in Java nachgebaut: viele Regeln über EINEN Strukturtyp je Seite und
 * EINEN Typ für die Knoten-Korrespondenz. Was die Regeln unterscheidet,
 * ist der Wert eines Blatts neben dem Knoten ({@code rule} links,
 * {@code feature} rechts), für die Hälfte der Arten auch der Wert des
 * Blatts am Elternknoten.
 *
 * <p>Regelsatz, Modell (derselbe Zufallsgenerator mit demselben
 * Startwert) und Ablauf jeder Zelle sind die der Rust-Seite, ebenso der
 * Fingerprint ({@code benches/suite/fingerprint.rs}): gleiche Zelle,
 * gleiche Schritte, gleiche Knotenzahl, gleicher Fingerprint heißt,
 * Java und Rust haben denselben Graphen mit derselben Kaskade in
 * derselben Reihenfolge erzeugt.
 *
 * <p>Öffentlich für die Messstrecke {@code bench.JavaSuite}. Jede Zelle
 * gibt es auch mit einer {@link Section}, die der Aufrufer anlegt und
 * nach dem Lauf ausliest. Die Messstrecke läuft gegen die Hauptklassen
 * älterer Stände; deshalb nutzt diese Klasse nur öffentliche API der
 * Engine und liest die paket-privaten Zähler über {@link Counters}.
 */
public final class Langpack {
    private Langpack() {}

    /** {@code kinds}: Zahl der Arten, der Regelsatz hat
     *  {@code 2 * kinds + 1} Regeln. {@code nodes}: Sprachknoten unter
     *  der Wurzel. */
    public record Spec(int kinds, int nodes) {}

    /** Ergebnis eines Laufs: Rückgaben von {@code step}, Einträge der
     *  Kaskade, Knoten des Graphen, Fingerprint, Wall-Zeit der
     *  Messstrecke, davon die Zeit des ersten Aufrufs ({@code seed} oder
     *  {@code elementsAdded}; 0, wo die Zelle sie nicht trennt), dazu
     *  die Zahl der gestarteten Suchen und der Seeds über den Index
     *  (ganzer Lauf, auch außerhalb der Messstrecke; -1, wo der Stand
     *  den Zähler nicht hat). */
    public record Outcome(long steps, long applied, long nodes, String fingerprint, long nanos,
            long firstNanos, long searches, long seedsByIndex) {}

    /** xorshift64 mit festem Startwert: dasselbe Modell wie in Rust. */
    static final class Rng {
        long s;

        Rng(long seed) { s = seed; }

        long next() {
            s ^= s << 13;
            s ^= s >>> 7;
            s ^= s << 17;
            return s;
        }

        int below(int n) {
            return (int) Long.remainderUnsigned(next(), n);
        }
    }

    private static String leaf(String name, String typ, String value, boolean context) {
        return "{\"name\":\"" + name + "\",\"type\":\"" + typ + "\",\"predicate\":{\"kind\":\"equals\","
                + "\"value\":\"" + value + "\"}," + (context ? "\"context\":true"
                : "\"constant\":\"" + value + "\"") + "}";
    }

    static String rulesJson(int kinds) {
        int half = kinds / 2;
        StringBuilder rules = new StringBuilder(LangWorld.root());
        for (int i = 0; i < kinds; i++) {
            String left = "{\"name\":\"sp\",\"type\":\"J\"},{\"name\":\"a\",\"type\":\"J\"},"
                    + leaf("a_rule", "rule", "k" + i, false);
            String links = "[\"sp\",\"a\"],[\"a\",\"a_rule\"]";
            if (i >= half) {
                left += "," + leaf("sp_rule", "rule", "k" + (i - half), true);
                links += ",[\"sp\",\"sp_rule\"]";
            }
            rules.append(",{\"name\":\"C").append(i).append("\",\"rank\":200,")
                    .append("\"left\":{\"anchor\":\"a\",\"nodes\":[").append(left)
                    .append("],\"links\":[").append(links).append("]},")
                    .append("\"right\":{\"anchor\":\"b\",\"nodes\":[{\"name\":\"sb\",\"type\":\"S\"},")
                    .append("{\"name\":\"b\",\"type\":\"S\"},")
                    .append(leaf("b_feature", "feature", "F" + i, false))
                    .append("],\"links\":[[\"sb\",\"b\"],[\"b\",\"b_feature\"]]},")
                    .append("\"corrs\":[{\"type\":\"CN\",\"left\":\"sp\",\"right\":\"sb\",\"role\":\"references\"},")
                    .append("{\"type\":\"CN\",\"left\":\"a\",\"right\":\"b\",\"role\":\"establishes\"}]}");
            rules.append(",{\"name\":\"A").append(i).append("\",\"rank\":100,")
                    .append("\"left\":{\"anchor\":\"al\",\"nodes\":[{\"name\":\"a\",\"type\":\"J\"},")
                    .append(leaf("a_rule", "rule", "k" + i, true)).append(',')
                    .append(leaf("al", "flag", "yes", false))
                    .append("],\"links\":[[\"a\",\"a_rule\"],[\"a\",\"al\"]]},")
                    .append("\"right\":{\"anchor\":\"bl\",\"nodes\":[{\"name\":\"b\",\"type\":\"S\"},")
                    .append(leaf("b_feature", "feature", "F" + i, true)).append(',')
                    .append(leaf("bl", "open", "on", false))
                    .append("],\"links\":[[\"b\",\"b_feature\"],[\"b\",\"bl\"]]},")
                    .append("\"corrs\":[{\"type\":\"CN\",\"left\":\"a\",\"right\":\"b\",\"role\":\"references\"},")
                    .append("{\"type\":\"CA\",\"left\":\"al\",\"right\":\"bl\",\"role\":\"establishes\"}]}");
        }
        return "{\"format\":3,\"name\":\"langpack\",\"rules\":[" + rules + "]}";
    }

    /** Ein Sprachknoten des Quellmodells. */
    private static final class Lang {
        final Id node, ruleLeaf;
        int kind;

        Lang(Id node, Id ruleLeaf, int kind) {
            this.node = node; this.ruleLeaf = ruleLeaf; this.kind = kind;
        }
    }

    /** Eltern kommen aus so vielen jüngsten Knoten: begrenzter
     *  Fächer, wachsende Tiefe. */
    private static final int WINDOW = 64;

    /** Ein Quellmodell: Projekt, Datei, Einheit und der Baum darunter. */
    static final class Model {
        final Graph g = new Graph();
        final Map<Id, String> vs = new HashMap<>();
        /** Jeder Baseline-Knoten, in der Reihenfolge des Anlegens. */
        final List<Id> all = new ArrayList<>();
        /** Erst die Wurzel, dann die Sprachknoten. */
        final List<Lang> lang = new ArrayList<>();
        /** Sprachknoten je Art. */
        final List<List<Integer>> byKind = new ArrayList<>();
        final Rng rng = new Rng(0x9E3779B97F4A7C15L);
        final Spec spec;

        Model(Spec spec) {
            this.spec = spec;
            g.addBaseline("project", "P");
            Id file = g.addBaseline("file", "F");
            Id root = g.addBaseline("n0", "J");
            Id leaf = g.addBaseline("n0/rule", "rule");
            vs.put(leaf, "unit");
            g.connect(file, root, St.SOLID);
            g.connect(root, leaf, St.SOLID);
            all.addAll(List.of(file, root, leaf));
            lang.add(new Lang(root, leaf, -1));
            for (int i = 0; i < spec.kinds(); i++) byKind.add(new ArrayList<>());
            for (int i = 0; i < spec.nodes(); i++) grow();
        }

        /** Fügt einen Sprachknoten mit seinen Blättern hinzu; liefert
         *  die neuen Baseline-Knoten. */
        List<Id> grow() {
            int half = spec.kinds() / 2;
            int ix = lang.size();
            int kind = rng.below(spec.kinds());
            int parent;
            if (kind >= half && !byKind.get(kind - half).isEmpty()) {
                List<Integer> ofKind = byKind.get(kind - half);
                int from = Math.max(0, ofKind.size() - WINDOW);
                parent = ofKind.get(from + rng.below(ofKind.size() - from));
            } else {
                if (kind >= half) kind -= half;
                int from = Math.max(0, ix - WINDOW);
                parent = from + rng.below(ix - from);
            }
            Id node = g.addBaseline("n" + ix, "J");
            g.connect(lang.get(parent).node, node, St.SOLID);
            Id ruleLeaf = g.addBaseline("n" + ix + "/rule", "rule");
            vs.put(ruleLeaf, "k" + kind);
            g.connect(node, ruleLeaf, St.SOLID);
            List<Id> fresh = new ArrayList<>(List.of(node, ruleLeaf));
            int flag = rng.below(3);
            if (flag > 0) {
                Id l = g.addBaseline("n" + ix + "/flag", "flag");
                vs.put(l, flag == 1 ? "yes" : "no");
                g.connect(node, l, St.SOLID);
                fresh.add(l);
            }
            lang.add(new Lang(node, ruleLeaf, kind));
            byKind.get(kind).add(ix);
            all.addAll(fresh);
            return fresh;
        }
    }

    private static long saturate(Engine e, Graph g, Map<Id, String> vs) {
        long steps = 0;
        while (e.step(g, vs) != null) steps++;
        return steps;
    }

    /** Ergebnis nach der beendeten Messstrecke {@code measured}. */
    public static Outcome outcome(Graph g, Engine e, Map<Id, String> vs, long steps,
            Section measured) {
        return outcome(g, e, vs, steps, measured, 0);
    }

    private static Outcome outcome(Graph g, Engine e, Map<Id, String> vs, long steps,
            Section measured, long firstNanos) {
        return new Outcome(steps, e.cascade.size(), g.allNodes().size(), fingerprint(g, e, vs),
                measured.wallNanos(), firstNanos, Counters.of(e, "searches"),
                Counters.of(e, "seedsByIndex"));
    }

    /** Batch über das ganze Modell durch {@code seed}. */
    static Outcome seed(Spec spec, boolean useIndex) {
        return seed(spec, useIndex, new Section());
    }

    public static Outcome seed(Spec spec, boolean useIndex, Section measured) {
        Model m = new Model(spec);
        List<Rule> rules = Rules.load(rulesJson(spec.kinds()), m.g);
        Engine e = new Engine(rules, useIndex);
        measured.start();
        e.seed(m.g, m.vs);
        long first = measured.elapsedNanos();
        long steps = saturate(e, m.g, m.vs);
        measured.stop();
        return outcome(m.g, e, m.vs, steps, measured, first);
    }

    /** Das ganze Modell als ein Add-Delta ({@code elementsAdded}). */
    static Outcome add(Spec spec, boolean useIndex) {
        return add(spec, useIndex, new Section());
    }

    public static Outcome add(Spec spec, boolean useIndex, Section measured) {
        Model m = new Model(spec);
        List<Rule> rules = Rules.load(rulesJson(spec.kinds()), m.g);
        Engine e = new Engine(rules, useIndex);
        measured.start();
        e.elementsAdded(m.g, m.vs, m.all);
        long first = measured.elapsedNanos();
        long steps = saturate(e, m.g, m.vs);
        measured.stop();
        return outcome(m.g, e, m.vs, steps, measured, first);
    }

    /** Summe der Zähler {@code name} der Engines, -1 wo ein Stand ihn
     *  nicht hat. */
    private static long sum(List<Engine> engines, String name) {
        long sum = 0;
        for (Engine e : engines) {
            long n = Counters.of(e, name);
            if (n < 0) return -1;
            sum += n;
        }
        return sum;
    }

    /**
     * {@code engines} Läufe nacheinander in einem Prozess, jeder mit
     * eigenem Modell, eigenem Regelsatz und eigener Engine (Rust
     * {@code langpack::engines}): ein Aufrufer, der je Eingabe die Regeln
     * lädt und eine Engine baut. Modelle und Regeln entstehen vor der
     * Messstrecke. Der Fingerprint deckt alle Läufe
     * ({@link #fingerprintOfText}), Schritte, angewandte Schritte, Knoten
     * und Suchen sind die Summen.
     */
    public static Outcome engines(Spec spec, int engines, boolean useIndex, Section measured) {
        List<Model> models = new ArrayList<>(engines);
        List<List<Rule>> ruleSets = new ArrayList<>(engines);
        for (int i = 0; i < engines; i++) {
            Model m = new Model(spec);
            ruleSets.add(Rules.load(rulesJson(spec.kinds()), m.g));
            models.add(m);
        }
        List<Engine> runs = new ArrayList<>(engines);
        measured.start();
        long steps = 0;
        for (int i = 0; i < engines; i++) {
            Model m = models.get(i);
            Engine e = new Engine(ruleSets.get(i), useIndex);
            e.seed(m.g, m.vs);
            steps += saturate(e, m.g, m.vs);
            runs.add(e);
        }
        measured.stop();
        StringBuilder all = new StringBuilder();
        long applied = 0;
        long nodes = 0;
        for (int i = 0; i < engines; i++) {
            Model m = models.get(i);
            Engine e = runs.get(i);
            applied += e.cascade.size();
            nodes += m.g.allNodes().size();
            all.append(fingerprint(m.g, e, m.vs));
        }
        return new Outcome(steps, applied, nodes, fingerprintOfText(all.toString()),
                measured.wallNanos(), 0, sum(runs, "searches"), sum(runs, "seedsByIndex"));
    }

    /**
     * {@code engines} Engines nacheinander, jede über neu geladene Regeln,
     * jede mit einer Expansion, die nichts findet (Rust
     * {@code langpack::index_only}): was es eine Engine kostet, zu ihrem
     * Index zu kommen. Alle Regelsätze liegen im selben Graphen, gemessen
     * werden Bau der Engines und Expansionen. Ergebnis der letzten Engine.
     */
    public static Outcome indexOnly(Spec spec, int engines, boolean useIndex, Section measured) {
        Graph g = new Graph();
        Map<Id, String> vs = new HashMap<>();
        Id lone = g.addBaseline("lone", "Lone");
        List<List<Rule>> ruleSets = new ArrayList<>(engines);
        for (int i = 0; i < engines; i++) ruleSets.add(Rules.load(rulesJson(spec.kinds()), g));
        List<Engine> runs = new ArrayList<>(engines);
        measured.start();
        for (List<Rule> rules : ruleSets) {
            Engine e = new Engine(rules, useIndex);
            e.elementsAdded(g, vs, List.of(lone));
            runs.add(e);
        }
        measured.stop();
        return outcome(g, runs.get(runs.size() - 1), vs, 0, measured);
    }

    /** Ein synchronisiertes Modell mit Engine, Ausgang der Delta-Zellen. */
    private static Engine synchronised(Model m, boolean useIndex) {
        List<Rule> rules = Rules.load(rulesJson(m.spec.kinds()), m.g);
        Engine e = new Engine(rules, useIndex);
        e.elementsAdded(m.g, m.vs, m.all);
        saturate(e, m.g, m.vs);
        return e;
    }

    /**
     * Eine Engine, die auf einem schon übersetzten Modell beginnt (Rust
     * {@code langpack::reseed}): eine zweite Engine über denselben Regeln
     * seedet das synchronisierte Modell, die Suchen auf {@code threads}
     * Threads ({@link Seeds#seed}, ein Thread ist {@code seed}). Gemessen
     * ist dieser Seed allein, er findet die Matches jeder Regel. Die
     * Schritte bis zur Sättigung folgen nach der Messstrecke.
     */
    public static Outcome reseed(Spec spec, int threads, boolean useIndex, Section measured) {
        Seeds.require(threads);
        Model m = new Model(spec);
        Engine first = synchronised(m, useIndex);
        Engine e = new Engine(first.rules, useIndex);
        measured.start();
        Seeds.seed(e, m.g, m.vs, threads);
        measured.stop();
        long steps = saturate(e, m.g, m.vs);
        return outcome(m.g, e, m.vs, steps, measured);
    }

    /** {@code delta} neue Knoten auf einem synchronisierten Modell, eine
     *  Welle je Knoten. */
    static Outcome deltaAdd(Spec spec, int delta, boolean useIndex) {
        return deltaAdd(spec, delta, useIndex, new Section());
    }

    public static Outcome deltaAdd(Spec spec, int delta, boolean useIndex, Section measured) {
        Model m = new Model(spec);
        Engine e = synchronised(m, useIndex);
        measured.start();
        long steps = 0;
        for (int i = 0; i < delta; i++) {
            List<Id> fresh = m.grow();
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementsAdded(m.g, m.vs, fresh);
            steps += saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        measured.stop();
        return outcome(m.g, e, m.vs, steps, measured);
    }

    /** {@code delta} Wertänderungen eines {@code rule}-Blatts (der
     *  Knoten wechselt seine Art), eine Welle je Änderung. */
    static Outcome deltaChange(Spec spec, int delta, boolean useIndex) {
        return deltaChange(spec, delta, useIndex, new Section());
    }

    public static Outcome deltaChange(Spec spec, int delta, boolean useIndex, Section measured) {
        Model m = new Model(spec);
        Engine e = synchronised(m, useIndex);
        int half = spec.kinds() / 2;
        measured.start();
        long steps = 0;
        for (int i = 0; i < delta; i++) {
            int ix = 1 + m.rng.below(m.lang.size() - 1);
            Lang l = m.lang.get(ix);
            // Eine andere Art ohne Elternbedingung.
            int kind = (l.kind % half + 1) % half;
            l.kind = kind;
            m.vs.put(l.ruleLeaf, "k" + kind);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(l.ruleLeaf);
            e.elementChanged(m.g, l.ruleLeaf);
            e.elementsAdded(m.g, m.vs, List.of(l.ruleLeaf));
            steps += saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        measured.stop();
        return outcome(m.g, e, m.vs, steps, measured);
    }

    /** {@code delta} Löschungen eines Sprachknotens, eine Welle je
     *  Löschung. */
    static Outcome deltaDelete(Spec spec, int delta, boolean useIndex) {
        return deltaDelete(spec, delta, useIndex, new Section());
    }

    public static Outcome deltaDelete(Spec spec, int delta, boolean useIndex, Section measured) {
        Model m = new Model(spec);
        Engine e = synchronised(m, useIndex);
        measured.start();
        long steps = 0;
        for (int i = 0; i < delta; i++) {
            int ix = 1 + m.rng.below(m.lang.size() - 1);
            Id node = m.lang.get(ix).node;
            Node n = m.g.node(node);
            if (n == null || !n.status.matchable()) continue;
            m.g.setNodeStatus(node, St.TOMBSTONE);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(node);
            e.elementDeleted(m.g, node);
            steps += saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        measured.stop();
        return outcome(m.g, e, m.vs, steps, measured);
    }

    private static String fullHex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) {
            sb.append(Character.forDigit((b >> 4) & 0xF, 16)).append(Character.forDigit(b & 0xF, 16));
        }
        return sb.toString();
    }

    /** Das Vorwärtsergebnis als Baseline eines zweiten Graphen, dann die
     *  ganze Zielseite als ein Add-Delta: die Rückwärtswelle. */
    static Outcome backward(Spec spec, boolean useIndex) {
        return backward(spec, useIndex, new Section());
    }

    public static Outcome backward(Spec spec, boolean useIndex, Section measured) {
        // Einmal vorwärts (nicht gemessen) für ein Zielmodell.
        Model m = new Model(spec);
        synchronised(m, useIndex);

        // Die Zielseite (Typen S, feature, open und das Projekt) als
        // Baseline-Knoten in einen frischen Graphen kopieren.
        Graph g = new Graph();
        Map<Id, String> vs = new HashMap<>();
        List<String> target = List.of("P", "S", "feature", "open");
        List<Node> nodes = new ArrayList<>();
        for (Node n : m.g.allNodes()) {
            if (n.status.matchable() && target.contains(m.g.typeName(n.typ))) nodes.add(n);
        }
        nodes.sort((x, y) -> x.id.compareTo(y.id));
        TreeMap<Id, Id> copy = new TreeMap<>();
        List<Id> all = new ArrayList<>();
        for (Node n : nodes) {
            Id id = g.addBaseline(fullHex(n.id), m.g.typeName(n.typ));
            String v = m.g.resolveValue(n.id, m.vs);
            if (v != null) vs.put(id, v);
            copy.put(n.id, id);
            all.add(id);
        }
        for (Node n : nodes) {
            for (Part p : m.g.parts(n.id)) {
                if (p.outgoing && m.g.connected(n.id, p.other)) {
                    Id to = copy.get(p.other);
                    if (to != null) g.connect(copy.get(n.id), to, St.SOLID);
                }
            }
        }
        List<Rule> rules = Rules.load(rulesJson(spec.kinds()), g);
        Engine e = new Engine(rules, useIndex);
        measured.start();
        e.admitDelta(List.of(Engine.DeltaDomain.TARGET));
        e.elementsAdded(g, vs, all);
        long first = measured.elapsedNanos();
        long steps = saturate(e, g, vs);
        measured.stop();
        return outcome(g, e, vs, steps, measured, first);
    }

    // ── Fingerprint wie benches/suite/fingerprint.rs ──

    private static void u64(Blake3 h, long v) {
        byte[] b = new byte[8];
        for (int i = 0; i < 8; i++) b[i] = (byte) (v >>> (8 * i));
        h.update(b);
    }

    private static void var(Blake3 h, String s) {
        byte[] b = s.getBytes(StandardCharsets.UTF_8);
        u64(h, b.length);
        h.update(b);
    }

    /** Der Name des Status, wie Rust ihn mit {@code {:?}} schreibt. */
    private static String status(St st) {
        return switch (st) {
            case SOLID -> "Solid";
            case GHOST -> "Ghost";
            case TENTATIVE_TOMBSTONE -> "TentativeTombstone";
            case TOMBSTONE -> "Tombstone";
        };
    }

    private record Edge(Id id, String status) {}

    /**
     * Hash über alle Knoten (Id, Typ, Status, Corr-Merkmal, Wert), alle
     * Verbindungen (Id, Status) und die Einträge der Kaskade in ihrer
     * Reihenfolge. Die ersten 16 Hex-Zeichen von blake3.
     */
    public static String fingerprint(Graph g, Engine e, Map<Id, String> vals) {
        return fingerprint(g, List.of(e), vals);
    }

    /**
     * Dasselbe über einen Graphen und mehrere Engines
     * ({@code fingerprint::of_all}): der Graph, dann die Kaskade jeder
     * Engine in der gegebenen Reihenfolge.
     */
    public static String fingerprint(Graph g, List<Engine> engines, Map<Id, String> vals) {
        Blake3 h = new Blake3();
        List<Node> nodes = g.allNodes();
        nodes.sort((x, y) -> x.id.compareTo(y.id));
        u64(h, nodes.size());
        List<Edge> connections = new ArrayList<>();
        for (Node n : nodes) {
            h.update(n.id.b);
            var(h, g.typeName(n.typ));
            var(h, status(n.status));
            h.update(new byte[] {(byte) (n.isCorr ? 1 : 0)});
            String v = g.resolveValue(n.id, vals);
            if (v == null) {
                h.update(new byte[] {0});
            } else {
                h.update(new byte[] {1});
                var(h, v);
            }
            for (Part p : g.parts(n.id)) {
                if (!p.outgoing) continue;
                Conn c = g.conn(p.connection);
                connections.add(new Edge(p.connection, c == null ? "" : status(c.status)));
            }
        }
        connections.sort((x, y) -> {
            int c = x.id().compareTo(y.id());
            return c != 0 ? c : x.status().compareTo(y.status());
        });
        u64(h, connections.size());
        for (Edge c : connections) {
            h.update(c.id().b);
            var(h, c.status());
        }
        for (Engine e : engines) {
            u64(h, e.cascade.size());
            for (Engine.Entry entry : e.cascade) {
                u64(h, entry.ruleIx);
                u64(h, entry.rank);
                u64(h, entry.refs.length);
                for (Id id : entry.refs) h.update(id.b);
                u64(h, entry.created.size());
                for (Id id : entry.created) h.update(id.b);
                u64(h, entry.createdEdges.size());
                for (Id id : entry.createdEdges) h.update(id.b);
            }
        }
        byte[] out = new byte[32];
        h.finalize32(out);
        return hex16(out);
    }

    /** Fingerprint mehrerer Läufe ({@code fingerprint::of_text}): der Hash
     *  über ihre Fingerprints in ihrer Folge. */
    static String fingerprintOfText(String fingerprints) {
        return hex16(Blake3.hash(fingerprints.getBytes(StandardCharsets.UTF_8)));
    }

    /** Die ersten 16 Hex-Zeichen eines Digests. */
    private static String hex16(byte[] digest) {
        StringBuilder sb = new StringBuilder(16);
        for (int i = 0; i < 8; i++) {
            sb.append(Character.forDigit((digest[i] >> 4) & 0xF, 16))
                    .append(Character.forDigit(digest[i] & 0xF, 16));
        }
        return sb.toString();
    }
}
