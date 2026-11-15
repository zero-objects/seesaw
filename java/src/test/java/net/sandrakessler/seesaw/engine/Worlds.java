package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Kleine typisierte Graphen mit jeder Art von Delta: Knoten, Links,
 * Werte, Löschungen auf beiden Seiten, Seeds, Läufe mit Schrittgrenze
 * und Wellen, die vor der Sättigung enden. Die Regelsätze und der
 * Ablauf einer Welle sind die des Rust-Differenztests
 * ({@code difftest/tests/graphs.rs}).
 *
 * <p>Ein Szenario läuft durch eine Engine mit und eine ohne
 * {@link AnchorIndex}. Nach jeder Welle wird ein Abzug verglichen:
 * Graph, Kaskade, die Folge der Rückgaben von {@code step} und der
 * Zustand der Engine (eingetragene Matches in ihrer Reihenfolge, To-do,
 * angewandte Matches).
 */
final class Worlds {
    private Worlds() {}

    /** Ein Knotentyp einer Welt. */
    record Kind(String name, String[] values, boolean target) {}

    /** Ein Regelsatz mit den Knotentypen und Links, die ein Modell dazu
     *  benutzt. {@code links}: (Art der Quelle, Art des Ziels). */
    record World(String name, String rules, Kind[] kinds, int[][] links) {}

    enum Op { ADD_NODE, ADD_LINK, REMOVE_LINK, DELETE_NODE, CHANGE_VALUE, DELETE_PRODUCT,
        DELETE_PRODUCT_EDGE, SEED, RUN }

    /** Eine Welle: das Delta, dann Schritte. {@code limit >= 0}:
     *  höchstens so viele Schritte und keine Konsolidierung. */
    static final class Wave {
        Op op;
        int a, b, c;
        int[][] links = new int[0][];
        int limit = -1;

        @Override public String toString() {
            StringBuilder sb = new StringBuilder(op.toString()).append('(').append(a).append(',')
                    .append(b).append(',').append(c);
            for (int[] l : links) sb.append(" [").append(l[0]).append(',').append(l[1]).append(']');
            return sb.append(") limit=").append(limit).toString();
        }
    }

    private static final String[] NONE = new String[0];
    private static final String[] NAMES = {"Ada", "Bob", "Cy"};

    static World followup() {
        String rules = """
            {"format":3,"name":"difftest_followup","rules":[
              {"name":"Root","rank":100,
               "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
               "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
               "corrs":[{"type":"AB","left":"a","right":"b","role":"establishes"}]},
              {"name":"Child","rank":90,
               "left":{"anchor":"a","nodes":[{"name":"a","type":"A"},
                       {"name":"x","type":"X"}],"links":[["a","x"]]},
               "right":{"anchor":"b","nodes":[{"name":"b","type":"B"},
                        {"name":"y","type":"Y"}],"links":[["b","y"]]},
               "corrs":[{"type":"AB","left":"a","right":"b","role":"references"},
                        {"type":"XY","left":"x","right":"y","role":"establishes"}]}
            ]}""";
        return new World("followup", rules, new Kind[] {
            new Kind("A", NONE, false), new Kind("X", NONE, false),
            new Kind("B", NONE, true), new Kind("Y", NONE, true),
        }, new int[][] {{0, 1}, {0, 1}, {2, 3}});
    }

    static World family() {
        StringBuilder rules = new StringBuilder("{\"format\":3,\"name\":\"difftest_family\",\"rules\":[");
        String[][] variants = {
            {"Father_2_Male", "850", "PersonCorr"},
            {"Father_2_Male_Variant", "800", "PersonCorr"},
            {"Father_2_Male_Other", "700", "OtherCorr"},
        };
        for (int i = 0; i < variants.length; i++) {
            if (i > 0) rules.append(',');
            rules.append("""
                {"name":"%s","rank":%s,
                 "left":{"anchor":"fam","nodes":[
                   {"name":"fam","type":"Family"},{"name":"father","type":"Father"},
                   {"name":"member","type":"Member"},{"name":"first","type":"firstName"}],
                   "links":[["fam","father"],["father","member"],["member","first"]]},
                 "right":{"anchor":"male","nodes":[
                   {"name":"male","type":"Male"},{"name":"name","type":"name"}],
                   "links":[["male","name"]]},
                 "corrs":[{"type":"%s","left":"member","right":"male","role":"establishes",
                   "bindings":[{"left":"first","right":"name"}]}]}"""
                    .formatted(variants[i][0], variants[i][1], variants[i][2]));
        }
        rules.append("]}");
        return new World("family", rules.toString(), new Kind[] {
            new Kind("Family", NONE, false), new Kind("Father", NONE, false),
            new Kind("Member", NONE, false), new Kind("firstName", NAMES, false),
            new Kind("Male", NONE, true), new Kind("name", NAMES, true),
        }, new int[][] {{0, 1}, {1, 2}, {2, 3}, {4, 5}});
    }

    /** Die Regeln der Welt {@link #predicates}: Name, Rang, Prädikat. */
    static final String[][] PREDICATE_RULES = {
        {"Equals", "500", "{\"kind\":\"equals\",\"value\":\"yes\"}"},
        {"Prefix", "400", "{\"kind\":\"prefix\",\"value\":\"get\"}"},
        {"Regex", "300", "{\"kind\":\"regex\",\"pattern\":\"Abstract.*\"}"},
        {"Range", "200", "{\"kind\":\"numeric_range\",\"min\":0.0,\"max\":100.0}"},
        {"Exists", "100", "{\"kind\":\"exists\"}"},
    };

    /** Der Regelsatz aus den Regeln {@code which} von
     *  {@link #PREDICATE_RULES}. */
    static String predicateRules(int... which) {
        StringBuilder rules = new StringBuilder("{\"format\":3,\"name\":\"difftest_predicates\",\"rules\":[");
        for (int i = 0; i < which.length; i++) {
            String[] rule = PREDICATE_RULES[which[i]];
            if (i > 0) rules.append(',');
            rules.append("""
                {"name":"%s","rank":%s,
                 "left":{"anchor":"item","nodes":[
                   {"name":"item","type":"Item"},
                   {"name":"attr","type":"Attr","context":true,"predicate":%s}],
                   "links":[["item","attr"]]},
                 "right":{"anchor":"marker","nodes":[{"name":"marker","type":"Marker"}],"links":[]},
                 "corrs":[{"type":"C","left":"item","right":"marker","role":"establishes"}]}"""
                    .formatted(rule[0], rule[1], rule[2]));
        }
        return rules.append("]}").toString();
    }

    /**
     * Je Art von Wertprädikat eine Regel, das Prädikat an einem
     * Kontextblatt, alle mit demselben Anker.
     *
     * <p>{@code withEquals = false} lässt die Regel mit {@code equals}
     * weg. Mit ihr liegen alle fünf Plätze an derselben Stelle des
     * Index, und der Wert des Knotens wird schon ihretwegen aufgelöst.
     * Ohne sie hängt das Auflösen allein an den anderen Arten.
     */
    static World predicates(boolean withEquals) {
        String rules = withEquals ? predicateRules(0, 1, 2, 3, 4) : predicateRules(1, 2, 3, 4);
        return new World(withEquals ? "predicates" : "predicates_no_equals", rules, new Kind[] {
            new Kind("Item", NONE, false),
            new Kind("Attr", new String[] {"yes", "getAge", "AbstractList", "42", "no", "setAge",
                "7000"}, false),
            new Kind("Marker", NONE, true),
        }, new int[][] {{0, 1}});
    }

    /** Zufällige Wellen, die Gewichte wie im Rust-Differenztest. */
    static List<Wave> scenario(Random rnd, int maxWaves) {
        int n = rnd.nextInt(maxWaves);
        List<Wave> waves = new ArrayList<>(n);
        for (int i = 0; i < n; i++) {
            Wave w = new Wave();
            int pick = rnd.nextInt(26);
            w.a = rnd.nextInt(32);
            w.b = rnd.nextInt(32);
            w.c = rnd.nextInt(32);
            if (pick < 8) {
                w.op = Op.ADD_NODE;
                w.links = new int[rnd.nextInt(4)][];
                for (int k = 0; k < w.links.length; k++) {
                    w.links[k] = new int[] {rnd.nextInt(32), rnd.nextInt(32)};
                }
            } else if (pick < 11) {
                w.op = Op.ADD_LINK;
            } else if (pick < 13) {
                w.op = Op.REMOVE_LINK;
            } else if (pick < 16) {
                w.op = Op.DELETE_NODE;
            } else if (pick < 19) {
                w.op = Op.CHANGE_VALUE;
            } else if (pick < 21) {
                w.op = Op.DELETE_PRODUCT;
            } else if (pick < 23) {
                w.op = Op.DELETE_PRODUCT_EDGE;
            } else if (pick < 25) {
                w.op = Op.SEED;
            } else {
                w.op = Op.RUN;
                w.a = rnd.nextInt(6);
            }
            if (rnd.nextInt(5) == 0) w.limit = rnd.nextInt(3);
            waves.add(w);
        }
        return waves;
    }

    static String hex(Id id) {
        StringBuilder sb = new StringBuilder(24);
        for (int i = 0; i < 12; i++) {
            sb.append(Character.forDigit((id.b[i] >> 4) & 0xF, 16))
                    .append(Character.forDigit(id.b[i] & 0xF, 16));
        }
        return sb.toString();
    }

    private static void ids(StringBuilder out, String name, Iterable<Id> ids) {
        out.append(' ').append(name).append("=[");
        for (Id id : ids) out.append(hex(id)).append(' ');
        out.append(']');
    }

    /**
     * Abzug nach einer Welle: Knoten (Id, Typ, Status, Corr-Merkmal,
     * Wert), Verbindungen (Id, Status), die Kaskade in ihrer
     * Reihenfolge, dazu der Zustand der Engine.
     */
    static String dump(Graph g, Map<Id, String> vals, Engine e) {
        StringBuilder out = new StringBuilder();
        List<Node> nodes = g.allNodes();
        nodes.sort((x, y) -> x.id.compareTo(y.id));
        List<String> connections = new ArrayList<>();
        for (Node n : nodes) {
            out.append("node ").append(hex(n.id)).append(' ').append(g.typeName(n.typ)).append(' ')
                    .append(n.status).append(" corr=").append(n.isCorr).append(" value=")
                    .append(g.resolveValue(n.id, vals)).append('\n');
            for (Part p : g.parts(n.id)) {
                if (!p.outgoing) continue;
                Conn c = g.conn(p.connection);
                connections.add(hex(p.connection) + " " + (c == null ? "" : c.status));
            }
        }
        connections.sort(null);
        for (String c : connections) out.append("connection ").append(c).append('\n');
        for (Engine.Entry entry : e.cascade) {
            out.append("entry rule=").append(entry.ruleIx).append(" rank=").append(entry.rank);
            ids(out, "refs", List.of(entry.refs));
            ids(out, "created", entry.created);
            ids(out, "edges", entry.createdEdges);
            out.append('\n');
        }
        out.append("contradiction=").append(e.sawContradiction).append('\n');
        // Zustand der Engine: mit und ohne Index muss er gleich sein,
        // weil dieselbe Folge von Matches eingetragen wird.
        for (int i = 0; i < e.matches.size(); i++) {
            Engine.MatchRec m = e.matches.get(i);
            out.append("match ").append(i).append(" rule=").append(m.ruleIx).append(" dead=")
                    .append(m.dead).append(" entry=").append(m.entry);
            ids(out, "refs", List.of(m.refs));
            out.append('\n');
        }
        for (Engine.TodoKey k : e.todo) {
            out.append("todo rule=").append(k.ruleIx).append(" rank=").append(k.rank);
            ids(out, "refs", List.of(k.refs));
            out.append('\n');
        }
        for (Engine.AppliedKey k : e.applied) {
            out.append("applied rule=").append(k.ruleIx);
            ids(out, "refs", List.of(k.refs));
            out.append('\n');
        }
        out.append("by_key=").append(e.byKey.size()).append('\n');
        return out.toString();
    }

    private static boolean alive(Graph g, Id id) {
        Node n = g.node(id);
        return n != null && n.status.matchable();
    }

    private static int pick(int selector, int len) {
        return selector % len;
    }

    /** Die Rückgaben von {@code step}, ein Buchstabe je Schritt. */
    static String steps(Engine e, Graph g, Map<Id, String> vals, int limit) {
        StringBuilder steps = new StringBuilder();
        while (steps.length() < limit) {
            Boolean r = e.step(g, vals);
            if (r == null) break;
            steps.append(r ? 'a' : 'd');
        }
        return steps.toString();
    }

    /** Fährt ein Szenario durch eine Engine; ein Abzug je Welle. */
    static List<String> run(World world, boolean routed, List<Wave> waves, boolean useIndex,
            boolean indexSeed) {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(world.rules(), g);
        Map<Id, String> vals = new HashMap<>();
        List<Id> nodes = new ArrayList<>();
        List<Integer> kindOf = new ArrayList<>();
        Engine e = new Engine(rules, useIndex);
        if (indexSeed) e.seedFactor = 0;
        List<String> dumps = new ArrayList<>();
        for (int waveIx = 0; waveIx < waves.size(); waveIx++) {
            Wave wave = waves.get(waveIx);
            String note = "";
            switch (wave.op) {
                case ADD_NODE -> {
                    int k = pick(wave.a, world.kinds().length);
                    Kind kind = world.kinds()[k];
                    Id id = g.addBaseline("n" + waveIx, kind.name());
                    if (kind.values().length > 0) {
                        vals.put(id, kind.values()[pick(wave.b, kind.values().length)]);
                    }
                    for (int[] l : wave.links) {
                        int[] link = world.links()[pick(l[0], world.links().length)];
                        int wanted;
                        if (link[0] == k) {
                            wanted = link[1];
                        } else if (link[1] == k) {
                            wanted = link[0];
                        } else {
                            continue;
                        }
                        List<Id> others = new ArrayList<>();
                        for (int i = 0; i < nodes.size(); i++) {
                            if (kindOf.get(i) == wanted && alive(g, nodes.get(i))) others.add(nodes.get(i));
                        }
                        if (others.isEmpty()) continue;
                        Id other = others.get(pick(l[1], others.size()));
                        if (link[0] == k) {
                            g.connect(id, other, St.SOLID);
                        } else {
                            g.connect(other, id, St.SOLID);
                        }
                    }
                    nodes.add(id);
                    kindOf.add(k);
                    admit(e, routed, kind.target());
                    e.elementsAdded(g, vals, List.of(id));
                }
                case ADD_LINK -> {
                    int[] link = world.links()[pick(wave.a, world.links().length)];
                    List<Id> sources = new ArrayList<>();
                    List<Id> targets = new ArrayList<>();
                    for (int i = 0; i < nodes.size(); i++) {
                        if (!alive(g, nodes.get(i))) continue;
                        if (kindOf.get(i) == link[0]) sources.add(nodes.get(i));
                        if (kindOf.get(i) == link[1]) targets.add(nodes.get(i));
                    }
                    if (!sources.isEmpty() && !targets.isEmpty()) {
                        Id a = sources.get(pick(wave.b, sources.size()));
                        Id b = targets.get(pick(wave.c, targets.size()));
                        if (!a.equals(b) && !g.connected(a, b)) {
                            g.connect(a, b, St.SOLID);
                            admit(e, routed, world.kinds()[link[0]].target());
                            e.linkAdded(g, vals, a, b);
                            note = "linked";
                        }
                    }
                }
                case REMOVE_LINK -> {
                    if (!nodes.isEmpty()) {
                        int i = pick(wave.a, nodes.size());
                        Id a = nodes.get(i);
                        List<Part> live = new ArrayList<>();
                        for (Part p : g.parts(a)) {
                            Conn c = g.conn(p.connection);
                            if (p.outgoing && c != null && c.status == St.SOLID) live.add(p);
                        }
                        if (!live.isEmpty()) {
                            Part p = live.get(pick(wave.b, live.size()));
                            g.setConnectionStatus(p.connection, St.TOMBSTONE);
                            admit(e, routed, world.kinds()[kindOf.get(i)].target());
                            e.linkRemoved(g, a, p.other);
                            note = "unlinked";
                        }
                    }
                }
                case DELETE_NODE -> {
                    if (!nodes.isEmpty()) {
                        int i = pick(wave.a, nodes.size());
                        Id id = nodes.get(i);
                        if (alive(g, id)) {
                            g.setNodeStatus(id, St.TOMBSTONE);
                            admit(e, routed, world.kinds()[kindOf.get(i)].target());
                            e.elementRemoved(id);
                            e.elementDeleted(g, id);
                            note = "deleted";
                        }
                    }
                }
                case CHANGE_VALUE -> {
                    List<Integer> valued = new ArrayList<>();
                    for (int i = 0; i < nodes.size(); i++) {
                        if (world.kinds()[kindOf.get(i)].values().length > 0 && alive(g, nodes.get(i))) {
                            valued.add(i);
                        }
                    }
                    if (!valued.isEmpty()) {
                        int i = valued.get(pick(wave.a, valued.size()));
                        Id id = nodes.get(i);
                        String[] values = world.kinds()[kindOf.get(i)].values();
                        vals.put(id, values[pick(wave.b, values.length)]);
                        admit(e, routed, world.kinds()[kindOf.get(i)].target());
                        e.elementRemoved(id);
                        e.elementChanged(g, id);
                        e.elementsAdded(g, vals, List.of(id));
                        note = "changed";
                    }
                }
                case DELETE_PRODUCT -> {
                    if (!e.cascade.isEmpty()) {
                        List<Id> created = e.cascade.get(pick(wave.a, e.cascade.size())).created;
                        if (!created.isEmpty()) {
                            Id id = created.get(pick(wave.b, created.size()));
                            if (alive(g, id)) {
                                g.setNodeStatus(id, St.TOMBSTONE);
                                admit(e, routed, true);
                                e.elementRemoved(id);
                                e.elementDeleted(g, id);
                                note = "product deleted";
                            }
                        }
                    }
                }
                case DELETE_PRODUCT_EDGE -> {
                    if (!e.cascade.isEmpty()) {
                        List<Id> edges = e.cascade.get(pick(wave.a, e.cascade.size())).createdEdges;
                        if (!edges.isEmpty()) {
                            Id id = edges.get(pick(wave.b, edges.size()));
                            Conn c = g.conn(id);
                            if (c != null && c.status.matchable()) {
                                g.setConnectionStatus(id, St.TOMBSTONE);
                                admit(e, routed, true);
                                e.elementRemoved(id);
                                e.elementDeleted(g, id);
                                note = "product edge deleted";
                            }
                        }
                    }
                }
                case SEED -> {
                    if (routed) e.admitInitial();
                    e.seed(g, vals);
                }
                case RUN -> {
                    if (routed) e.admitInitial();
                    note = e.run(g, vals, wave.a).toString();
                }
                default -> throw new IllegalStateException();
            }
            String steps = steps(e, g, vals, wave.limit >= 0 ? wave.limit : 2000);
            if (wave.limit < 0) e.consolidate(g);
            dumps.add(wave + " " + note + "\nsteps=" + steps + "\n" + dump(g, vals, e));
        }
        return dumps;
    }

    private static void admit(Engine e, boolean routed, boolean target) {
        if (routed) {
            e.admitDelta(List.of(target ? Engine.DeltaDomain.TARGET : Engine.DeltaDomain.SOURCE));
        }
    }
}
