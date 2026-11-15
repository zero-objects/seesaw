package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/** Einzelfälle des {@link AnchorIndex}. */
final class AnchorIndexTest {

    /** Die Namen der Regeln der eingetragenen Matches, in ihrer
     *  Reihenfolge. */
    private static List<String> recorded(Engine e) {
        List<String> out = new ArrayList<>();
        for (Engine.MatchRec m : e.matches) out.add(e.rules.get(m.ruleIx).name);
        return out;
    }

    /**
     * Ein Blatt mit einem Wertprädikat kommt ALLEIN als neuer Knoten:
     * es steht dann selbst an der verankerten Position, und der Index
     * muss sein Prädikat gegen den aufgelösten Wert prüfen. Für jede Art
     * außer {@code equals} ging das auf der Rust-Seite einmal gegen
     * einen nicht aufgelösten Wert und verwarf den Match.
     *
     * <p>Je Art ein Regelsatz mit nur dieser Regel (neben einer Regel
     * mit {@code equals} wird der Wert schon ihretwegen aufgelöst), dazu
     * der Regelsatz mit allen fünf.
     */
    @Test
    void jedeArtVonPraedikatAmVerankertenKnoten() {
        String[] values = {"yes", "getAge", "AbstractList", "42", "no", "7000"};
        // Je Regel von PREDICATE_RULES: die Werte, bei denen sie zutrifft.
        boolean[][] holds = {
            {true, false, false, false, false, false},
            {false, true, false, false, false, false},
            {false, false, true, false, false, false},
            {false, false, false, true, false, false},
            {true, true, true, true, true, true},
        };
        int[][] sets = {{0}, {1}, {2}, {3}, {4}, {0, 1, 2, 3, 4}};
        for (int[] set : sets) {
            for (int v = 0; v < values.length; v++) {
                List<String> want = new ArrayList<>();
                for (int rule : set) {
                    if (holds[rule][v]) want.add(Worlds.PREDICATE_RULES[rule][0] + "→");
                }
                for (boolean useIndex : new boolean[] {false, true}) {
                    Graph g = new Graph();
                    List<Rule> rules = Rules.load(Worlds.predicateRules(set), g);
                    Map<Id, String> vals = new HashMap<>();
                    Id item = g.addBaseline("item", "Item");
                    Id attr = g.addBaseline("item/attr", "Attr");
                    g.connect(item, attr, St.SOLID);
                    vals.put(attr, values[v]);
                    Engine e = new Engine(rules, useIndex);
                    e.elementsAdded(g, vals, List.of(attr));
                    assertEquals(want, recorded(e), "Regeln " + java.util.Arrays.toString(set)
                            + ", Wert " + values[v] + ", Index " + useIndex);
                }
            }
        }
    }

    /** Ein Blatt ohne Wert: kein Prädikat trifft zu, auch
     *  {@code exists} nicht. Kein Match, mit und ohne Index. */
    @Test
    void blattOhneWertTraegtKeinPraedikat() {
        for (boolean useIndex : new boolean[] {false, true}) {
            Graph g = new Graph();
            List<Rule> rules = Rules.load(Worlds.predicates(true).rules(), g);
            Id item = g.addBaseline("item", "Item");
            Id attr = g.addBaseline("item/attr", "Attr");
            g.connect(item, attr, St.SOLID);
            Engine e = new Engine(rules, useIndex);
            e.elementsAdded(g, Map.of(), List.of(item, attr));
            assertEquals(List.of(), recorded(e), "Index " + useIndex);
        }
    }

    /** Ein Knoten mit vielen Nachbarn als ein Delta: mehr Nachbarn, als
     *  die Menge der besuchten Paare von einem Gang zum naechsten
     *  behaelt. */
    @Test
    void knotenMitVielenNachbarn() {
        List<List<String>> both = new ArrayList<>();
        for (boolean useIndex : new boolean[] {false, true}) {
            Graph g = new Graph();
            Map<Id, String> vals = new HashMap<>();
            g.addBaseline("project", "P");
            Id file = g.addBaseline("file", "F");
            Id root = g.addBaseline("n0", "J");
            Id rootRule = g.addBaseline("n0/rule", "rule");
            vals.put(rootRule, "unit");
            g.connect(file, root, St.SOLID);
            g.connect(root, rootRule, St.SOLID);
            List<Rule> rules = Rules.load(LangWorld.rulesJson(false), g);
            List<Id> all = new ArrayList<>(List.of(file, root, rootRule));
            String[] values = {"cls", "m", "x"};
            for (int i = 1; i <= 1500; i++) {
                Id node = g.addBaseline("n" + i, "J");
                g.connect(root, node, St.SOLID);
                Id leaf = g.addBaseline("n" + i + "/rule", "rule");
                vals.put(leaf, values[i % 3]);
                g.connect(node, leaf, St.SOLID);
                all.add(node);
                all.add(leaf);
            }
            Engine e = new Engine(rules, useIndex);
            e.elementsAdded(g, vals, all);
            String steps = Worlds.steps(e, g, vals, 100);
            both.add(List.of(steps, Worlds.dump(g, vals, e)));
        }
        assertEquals(both.get(0), both.get(1));
        assertTrue(both.get(0).get(0).contains("a"), "kein Schritt angewandt");
    }

    /** Ein vom Host gebauter Index wird für gleiche Muster genommen,
     *  für andere nicht. Das Ergebnis ist in beiden Fällen dasselbe. */
    @Test
    void geteilterIndexNurFuerGleicheMuster() {
        Graph g0 = new Graph();
        List<Rule> lang = Rules.load(LangWorld.rulesJson(false), g0);
        AnchorIndex index = AnchorIndex.build(lang);
        assertTrue(index.fits(lang));

        // Derselbe Regelsatz, gegen einen zweiten Graphen gesenkt, der
        // die Typen in derselben Reihenfolge kennt.
        Graph g1 = new Graph();
        List<Rule> again = Rules.load(LangWorld.rulesJson(false), g1);
        assertTrue(index.fits(again));

        Graph g2 = new Graph();
        List<Rule> other = Rules.load(LangWorld.rulesJson(true), g2);
        assertFalse(index.fits(other));
        assertFalse(index.fits(again.subList(0, again.size() - 1)));

        LangWorld.Scenario s = LangWorld.scenario(new java.util.Random(7), false, 16, 12, false, false);
        List<String> plain = LangWorld.run(s, false, false);
        assertEquals(plain, LangWorld.run(s, true, true));
    }

    /** Mit einem geteilten Index fahren zwei Engines dasselbe wie mit
     *  eigenem. */
    @Test
    void zweiEnginesEinIndex() {
        List<String> dumps = new ArrayList<>();
        AnchorIndex shared = null;
        for (int round = 0; round < 3; round++) {
            Graph g = new Graph();
            Map<Id, String> vals = new HashMap<>();
            List<Rule> rules = Rules.load(Worlds.followup().rules(), g);
            if (shared == null) shared = AnchorIndex.build(rules);
            Id a = g.addBaseline("a", "A");
            Id x = g.addBaseline("a/x", "X");
            g.connect(a, x, St.SOLID);
            // Runde 0: ohne Index. Runde 1 und 2: derselbe geteilte Index.
            Engine e = round == 0 ? new Engine(rules, false) : new Engine(rules, shared);
            e.elementsAdded(g, vals, List.of(a, x));
            String steps = Worlds.steps(e, g, vals, 100);
            dumps.add(steps + "\n" + Worlds.dump(g, vals, e));
        }
        assertEquals(dumps.get(0), dumps.get(1));
        assertEquals(dumps.get(0), dumps.get(2));
    }

    /** Regeln jenseits der Kodierung des Index: die Engine sucht dann
     *  ungefiltert, ohne Fehler. */
    @Test
    void zuGrossesMusterFaelltAufDieUngefilterteSucheZurueck() {
        Graph g = new Graph();
        int typ = g.intern("T");
        Rule r = new Rule();
        r.name = "Riese";
        r.rank = 1;
        r.patNodes = new ArrayList<>();
        for (int i = 0; i <= AnchorIndex.MAX_POSITIONS; i++) r.patNodes.add(new PatNode(typ, null));
        r.patLinks = new ArrayList<>();
        r.createNodes = new ArrayList<>();
        r.createLinks = new ArrayList<>();
        r.inputTypes = new ArrayList<>();
        List<Rule> rules = List.of(r);
        assertFalse(AnchorIndex.supports(rules));
        // Ein Knoten eines anderen Typs: die ungefilterte Schleife
        // findet keine Position fuer ihn.
        Id n = g.addBaseline("n", "U");
        Engine e = new Engine(rules, true);
        e.elementsAdded(g, Map.of(), List.of(n));
        e.seed(g, Map.of());
        assertEquals(0, e.matches.size());
    }
}
