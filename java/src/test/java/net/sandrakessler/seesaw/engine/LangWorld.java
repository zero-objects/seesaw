package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Regelformen eines Sprachpacks: ein Strukturtyp je Seite, ein Typ für
 * die Knoten-Korrespondenz, die Regeln unterscheiden sich durch
 * Blattwerte. Der Regelsatz hat Regeln, die um denselben Knoten
 * konkurrieren (eine mit Elternbedingung und eine ohne, zwei mit
 * derselben linken Seite), so dass Matches als Duplikat unterdrückt
 * werden und wieder anwendbar werden, wenn die konkurrierende
 * Übersetzung zurückgezogen wird. Regeln und Ablauf sind die von
 * {@code difftest/tests/lang.rs}.
 *
 * <p>Ein Szenario ist ein Baum, dann eine Folge von Wellen: einen
 * Knoten hinzufügen, einen Blattwert ändern, einen Knoten oder ein
 * Blatt löschen, ein Erzeugnis einer Regel löschen, neu seeden.
 */
final class LangWorld {
    private LangWorld() {}

    /** Ein Sprachknoten des Quellmodells. {@code flag}: 0 = kein
     *  Blatt, 1 = "yes", 2 = "no". */
    record NodeSpec(String rule, int parent, int flag) {}

    enum Op { ADD, CHANGE_RULE, CHANGE_FLAG, DELETE_NODE, DELETE_FLAG, DELETE_PRODUCT,
        DELETE_PRODUCT_EDGE, SEED }

    static final class Step {
        Op op;
        NodeSpec spec;
        int a, b;
        String value;
        boolean yes;

        @Override public String toString() {
            return op + "(" + spec + "," + a + "," + b + "," + value + "," + yes + ")";
        }
    }

    record Scenario(boolean lateContext, List<NodeSpec> initial, boolean bySeed, boolean routed,
            List<Step> ops) {}

    private static String leaf(String name, String typ, String value, boolean context) {
        return "{\"name\":\"" + name + "\",\"type\":\"" + typ + "\",\"predicate\":{\"kind\":\"equals\","
                + "\"value\":\"" + value + "\"}," + (context ? "\"context\":true"
                : "\"constant\":\"" + value + "\"") + "}";
    }

    /** Eine Kindregel: ein Knoten mit Regelwert {@code rule} unter
     *  irgendeinem Knoten oder unter einem mit Regelwert
     *  {@code parentRule}. */
    private static String child(String name, int rank, String parentRule, String rule,
            String feature) {
        String left = "{\"name\":\"sp\",\"type\":\"J\"},{\"name\":\"a\",\"type\":\"J\"},"
                + leaf("a_rule", "rule", rule, false);
        String links = "[\"sp\",\"a\"],[\"a\",\"a_rule\"]";
        if (parentRule != null) {
            left += "," + leaf("sp_rule", "rule", parentRule, true);
            links += ",[\"sp\",\"sp_rule\"]";
        }
        return "{\"name\":\"" + name + "\",\"rank\":" + rank + ","
                + "\"left\":{\"anchor\":\"a\",\"nodes\":[" + left + "],\"links\":[" + links + "]},"
                + "\"right\":{\"anchor\":\"b\",\"nodes\":[{\"name\":\"sb\",\"type\":\"S\"},"
                + "{\"name\":\"b\",\"type\":\"S\"}," + leaf("b_feature", "feature", feature, false)
                + "],\"links\":[[\"sb\",\"b\"],[\"b\",\"b_feature\"]]},"
                + "\"corrs\":[{\"type\":\"CN\",\"left\":\"sp\",\"right\":\"sb\",\"role\":\"references\"},"
                + "{\"type\":\"CN\",\"left\":\"a\",\"right\":\"b\",\"role\":\"establishes\"}]}";
    }

    /** Eine Flag-Regel: das Flag-Blatt eines Knotens, mit oder ohne
     *  Bedingung an den Regelwert des Knotens. */
    private static String flag(String name, int rank, String rule, String feature) {
        String left = "{\"name\":\"a\",\"type\":\"J\"}";
        String leftLinks = "";
        String right = "{\"name\":\"b\",\"type\":\"S\"}";
        String rightLinks = "";
        if (rule != null) {
            left += "," + leaf("a_rule", "rule", rule, true);
            leftLinks += "[\"a\",\"a_rule\"],";
            right += "," + leaf("b_feature", "feature", feature, true);
            rightLinks += "[\"b\",\"b_feature\"],";
        }
        left += "," + leaf("al", "flag", "yes", false);
        leftLinks += "[\"a\",\"al\"]";
        right += "," + leaf("bl", "open", "on", false);
        rightLinks += "[\"b\",\"bl\"]";
        return "{\"name\":\"" + name + "\",\"rank\":" + rank + ","
                + "\"left\":{\"anchor\":\"al\",\"nodes\":[" + left + "],\"links\":[" + leftLinks + "]},"
                + "\"right\":{\"anchor\":\"bl\",\"nodes\":[" + right + "],\"links\":[" + rightLinks + "]},"
                + "\"corrs\":[{\"type\":\"CN\",\"left\":\"a\",\"right\":\"b\",\"role\":\"references\"},"
                + "{\"type\":\"CA\",\"left\":\"al\",\"right\":\"bl\",\"role\":\"establishes\"}]}";
    }

    static String root() {
        return "{\"name\":\"Root\",\"rank\":900,"
                + "\"left\":{\"anchor\":\"n\",\"nodes\":[{\"name\":\"f\",\"type\":\"F\"},"
                + "{\"name\":\"n\",\"type\":\"J\"}," + leaf("n_rule", "rule", "unit", false)
                + "],\"links\":[[\"f\",\"n\"],[\"n\",\"n_rule\"]]},"
                + "\"right\":{\"anchor\":\"m\",\"nodes\":[{\"name\":\"p\",\"type\":\"P\",\"context\":true},"
                + "{\"name\":\"m\",\"type\":\"S\"}," + leaf("m_feature", "feature", "Unit", false)
                + "],\"links\":[[\"p\",\"m\"],[\"m\",\"m_feature\"]]},"
                + "\"corrs\":[{\"type\":\"CN\",\"left\":\"n\",\"right\":\"m\",\"role\":\"establishes\"},"
                + "{\"type\":\"CF\",\"left\":\"f\",\"right\":\"m\",\"role\":\"establishes\"}]}";
    }

    /**
     * {@code lateContext} fügt die Regel {@code Adopt} hinzu: ein
     * Knoten unter einem übersetzten Elternknoten wird übersetzt, wenn
     * ein Geschwister es ist. Ihr Match entsteht, wenn der Schritt des
     * Geschwisters dessen Korrespondenz anlegt, zu einer Zeit, in der
     * der Knoten selbst schon von einer anderen Regel übersetzt sein
     * kann.
     */
    static String rulesJson(boolean lateContext) {
        List<String> rules = new ArrayList<>(List.of(
                root(),
                child("Cls", 200, null, "cls", "Class"),
                // Konkurriert mit MethodAny um eine Methode unter einer Klasse.
                child("Method", 200, "cls", "m", "Method"),
                child("MethodAny", 150, null, "m", "Free"),
                child("Other", 200, null, "x", "Other"),
                // Konkurriert mit FlagAny um das Flag einer Methode.
                flag("Flag", 100, "m", "Method"),
                flag("FlagAny", 50, null, null)));
        if (lateContext) {
            rules.add("{\"name\":\"Adopt\",\"rank\":40,"
                    + "\"left\":{\"anchor\":\"a\",\"nodes\":[{\"name\":\"sp\",\"type\":\"J\"},"
                    + "{\"name\":\"a\",\"type\":\"J\"},{\"name\":\"sib\",\"type\":\"J\"}],"
                    + "\"links\":[[\"sp\",\"a\"],[\"sp\",\"sib\"]]},"
                    + "\"right\":{\"anchor\":\"b\",\"nodes\":[{\"name\":\"sb\",\"type\":\"S\"},"
                    + "{\"name\":\"b\",\"type\":\"S\"},{\"name\":\"x\",\"type\":\"S\"},"
                    + leaf("b_feature", "feature", "Adopted", false)
                    + "],\"links\":[[\"sb\",\"b\"],[\"sb\",\"x\"],[\"b\",\"b_feature\"]]},"
                    + "\"corrs\":[{\"type\":\"CN\",\"left\":\"sp\",\"right\":\"sb\",\"role\":\"references\"},"
                    + "{\"type\":\"CN\",\"left\":\"sib\",\"right\":\"x\",\"role\":\"references\"},"
                    + "{\"type\":\"CN\",\"left\":\"a\",\"right\":\"b\",\"role\":\"establishes\"}]}");
        }
        return "{\"format\":3,\"name\":\"difftest_lang\",\"rules\":[" + String.join(",", rules) + "]}";
    }

    private static final String[] RULE_VALUES = {"cls", "m", "x", "unit"};

    /** {@code hub}: die meisten Knoten hängen am Wurzelknoten, der
     *  damit viele Nachbarn bekommt. */
    private static NodeSpec nodeSpec(Random rnd, boolean hub) {
        int parent = hub && rnd.nextInt(8) != 0 ? 0 : rnd.nextInt(64);
        return new NodeSpec(RULE_VALUES[rnd.nextInt(4)], parent, rnd.nextInt(3));
    }

    static Scenario scenario(Random rnd, boolean lateContext, int nodes, int ops, boolean hub,
            boolean addsOnly) {
        List<NodeSpec> initial = new ArrayList<>();
        int n = rnd.nextInt(nodes);
        for (int i = 0; i < n; i++) initial.add(nodeSpec(rnd, hub));
        boolean bySeed = rnd.nextBoolean();
        boolean routed = rnd.nextBoolean();
        List<Step> steps = new ArrayList<>();
        int count = rnd.nextInt(ops);
        for (int i = 0; i < count; i++) {
            Step s = new Step();
            s.a = rnd.nextInt(64);
            s.b = rnd.nextInt(64);
            int pick = addsOnly ? 0 : rnd.nextInt(17);
            if (pick < 3) {
                s.op = Op.ADD;
                s.spec = nodeSpec(rnd, hub);
            } else if (pick < 7) {
                s.op = Op.CHANGE_RULE;
                s.value = RULE_VALUES[rnd.nextInt(4)];
            } else if (pick < 9) {
                s.op = Op.CHANGE_FLAG;
                s.yes = rnd.nextBoolean();
            } else if (pick < 11) {
                s.op = Op.DELETE_NODE;
            } else if (pick < 12) {
                s.op = Op.DELETE_FLAG;
            } else if (pick < 14) {
                s.op = Op.DELETE_PRODUCT;
            } else if (pick < 16) {
                s.op = Op.DELETE_PRODUCT_EDGE;
            } else {
                s.op = Op.SEED;
            }
            steps.add(s);
        }
        return new Scenario(lateContext, initial, bySeed, routed, steps);
    }

    private static final class Lang {
        final Id node, ruleLeaf, flagLeaf;

        Lang(Id node, Id ruleLeaf, Id flagLeaf) {
            this.node = node; this.ruleLeaf = ruleLeaf; this.flagLeaf = flagLeaf;
        }
    }

    private static boolean alive(Graph g, Id id) {
        Node n = g.node(id);
        return n != null && n.status.matchable();
    }

    /** Fügt einen Sprachknoten hinzu; liefert die neuen Knoten. */
    private static List<Id> grow(Graph g, Map<Id, String> vals, List<Lang> lang, NodeSpec spec) {
        int ix = lang.size();
        Id parent = lang.get(spec.parent() % ix).node;
        Id node = g.addBaseline("n" + ix, "J");
        g.connect(parent, node, St.SOLID);
        Id ruleLeaf = g.addBaseline("n" + ix + "/rule", "rule");
        vals.put(ruleLeaf, spec.rule());
        g.connect(node, ruleLeaf, St.SOLID);
        List<Id> fresh = new ArrayList<>(List.of(node, ruleLeaf));
        Id flagLeaf = null;
        if (spec.flag() > 0) {
            flagLeaf = g.addBaseline("n" + ix + "/flag", "flag");
            vals.put(flagLeaf, spec.flag() == 1 ? "yes" : "no");
            g.connect(node, flagLeaf, St.SOLID);
            fresh.add(flagLeaf);
        }
        lang.add(new Lang(node, ruleLeaf, flagLeaf));
        return fresh;
    }

    /** Schritte bis zur Sättigung, dann Konsolidierung. */
    private static String settle(Engine e, Graph g, Map<Id, String> vals) {
        String steps = Worlds.steps(e, g, vals, 2001);
        if (steps.length() > 2000) return steps + "!";
        e.consolidate(g);
        return steps;
    }

    /** Fährt ein Szenario durch eine Engine; ein Abzug je Welle. */
    static List<String> run(Scenario s, boolean useIndex, boolean indexSeed) {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        g.addBaseline("project", "P");
        Id file = g.addBaseline("file", "F");
        Id root = g.addBaseline("n0", "J");
        Id rootRule = g.addBaseline("n0/rule", "rule");
        vals.put(rootRule, "unit");
        g.connect(file, root, St.SOLID);
        g.connect(root, rootRule, St.SOLID);
        List<Rule> rules = Rules.load(rulesJson(s.lateContext()), g);
        List<Lang> lang = new ArrayList<>(List.of(new Lang(root, rootRule, null)));
        List<Id> all = new ArrayList<>(List.of(file, root, rootRule));
        for (NodeSpec spec : s.initial()) all.addAll(grow(g, vals, lang, spec));
        Engine e = new Engine(rules, useIndex);
        if (indexSeed) e.seedFactor = 0;
        if (s.bySeed()) {
            e.seed(g, vals);
        } else {
            source(e, s);
            e.elementsAdded(g, vals, all);
        }
        List<String> dumps = new ArrayList<>();
        dumps.add("initial\nsteps=" + settle(e, g, vals) + "\n" + Worlds.dump(g, vals, e));
        for (Step op : s.ops()) {
            switch (op.op) {
                case ADD -> {
                    List<Id> fresh = grow(g, vals, lang, op.spec);
                    source(e, s);
                    e.elementsAdded(g, vals, fresh);
                }
                case CHANGE_RULE -> {
                    Lang l = lang.get(op.a % lang.size());
                    if (alive(g, l.node) && alive(g, l.ruleLeaf)) {
                        vals.put(l.ruleLeaf, op.value);
                        source(e, s);
                        e.elementRemoved(l.ruleLeaf);
                        e.elementChanged(g, l.ruleLeaf);
                        e.elementsAdded(g, vals, List.of(l.ruleLeaf));
                    }
                }
                case CHANGE_FLAG -> {
                    Lang l = lang.get(op.a % lang.size());
                    if (l.flagLeaf != null && alive(g, l.node) && alive(g, l.flagLeaf)) {
                        vals.put(l.flagLeaf, op.yes ? "yes" : "no");
                        source(e, s);
                        e.elementRemoved(l.flagLeaf);
                        e.elementChanged(g, l.flagLeaf);
                        e.elementsAdded(g, vals, List.of(l.flagLeaf));
                    }
                }
                case DELETE_NODE -> {
                    int ix = op.a % lang.size();
                    Id node = lang.get(ix).node;
                    if (ix > 0 && alive(g, node)) {
                        g.setNodeStatus(node, St.TOMBSTONE);
                        source(e, s);
                        e.elementRemoved(node);
                        e.elementDeleted(g, node);
                    }
                }
                case DELETE_FLAG -> {
                    Id leaf = lang.get(op.a % lang.size()).flagLeaf;
                    if (leaf != null && alive(g, leaf)) {
                        g.setNodeStatus(leaf, St.TOMBSTONE);
                        source(e, s);
                        e.elementRemoved(leaf);
                        e.elementDeleted(g, leaf);
                    }
                }
                case DELETE_PRODUCT -> {
                    if (!e.cascade.isEmpty()) {
                        List<Id> created = e.cascade.get(op.a % e.cascade.size()).created;
                        if (!created.isEmpty()) {
                            Id id = created.get(op.b % created.size());
                            if (alive(g, id)) {
                                g.setNodeStatus(id, St.TOMBSTONE);
                                if (s.routed()) e.admitDelta(List.of(Engine.DeltaDomain.TARGET));
                                e.elementRemoved(id);
                                e.elementDeleted(g, id);
                            }
                        }
                    }
                }
                case DELETE_PRODUCT_EDGE -> {
                    if (!e.cascade.isEmpty()) {
                        List<Id> edges = e.cascade.get(op.a % e.cascade.size()).createdEdges;
                        if (!edges.isEmpty()) {
                            Id id = edges.get(op.b % edges.size());
                            Conn c = g.conn(id);
                            if (c != null && c.status.matchable()) {
                                g.setConnectionStatus(id, St.TOMBSTONE);
                                if (s.routed()) e.admitDelta(List.of(Engine.DeltaDomain.TARGET));
                                e.elementRemoved(id);
                                e.elementDeleted(g, id);
                            }
                        }
                    }
                }
                case SEED -> {
                    if (s.routed()) e.admitInitial();
                    e.seed(g, vals);
                }
                default -> throw new IllegalStateException();
            }
            dumps.add(op + "\nsteps=" + settle(e, g, vals) + "\n" + Worlds.dump(g, vals, e));
        }
        return dumps;
    }

    private static void source(Engine e, Scenario s) {
        if (s.routed()) e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
    }
}
