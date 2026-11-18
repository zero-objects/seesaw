package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
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

    /**
     * Ein Match, der mehrere neue Knoten bindet, wird nur vom ersten aus
     * gesucht (Rust 878b85f), und der Gang betritt die früheren Knoten
     * des Deltas nicht (Rust 17b7e52). Die Vorwärtsregel hat die
     * Positionen {@code item} und {@code attr} mit dem Link item→attr:
     * mit dem Delta {@code [item, attr]} findet die Suche an {@code item}
     * den Match, der Gang von {@code attr} betritt {@code item} nicht und
     * bietet die Regel an {@code attr} nicht an. Mit {@code [attr, item]}
     * umgekehrt. Die Rückwärtsregel hat {@code attr} als Kontext ohne
     * Link zu ihren übrigen Positionen, ein Platz ohne Pfad, Kandidat für
     * jedes {@code attr}. Zwei Suchen statt drei. Ein Knoten, der im
     * Delta ein zweites Mal steht, hat keine Tripel. Eingetragen werden
     * dieselben Matches in derselben Folge wie ungefiltert.
     */
    @Test
    void fruehereKnotenDesDeltasAusgelassen() {
        String[][] deltas = {
            {"item", "attr"}, {"attr", "item"}, {"item", "attr", "item"},
            {"attr", "attr", "item", "item"},
        };
        for (String[] delta : deltas) {
            List<List<String>> both = new ArrayList<>();
            for (boolean useIndex : new boolean[] {false, true}) {
                Graph g = new Graph();
                List<Rule> rules = Rules.load(Worlds.predicateRules(0), g);
                Map<Id, String> vals = new HashMap<>();
                Id item = g.addBaseline("item", "Item");
                Id attr = g.addBaseline("item/attr", "Attr");
                g.connect(item, attr, St.SOLID);
                vals.put(attr, "yes");
                List<Id> nodes = new ArrayList<>();
                for (String n : delta) nodes.add(n.equals("item") ? item : attr);
                Engine e = new Engine(rules, useIndex);
                e.elementsAdded(g, vals, nodes);
                if (useIndex) {
                    assertEquals(2, e.anchoredSearches,
                            "verankerte Suchen, Delta " + java.util.Arrays.toString(delta));
                }
                both.add(List.of(String.join(" ", recorded(e)), Worlds.steps(e, g, vals, 100),
                        Worlds.dump(g, vals, e)));
            }
            assertEquals(both.get(0), both.get(1), "Delta " + java.util.Arrays.toString(delta));
            assertTrue(both.get(0).get(1).contains("a"), "kein Schritt angewandt");
        }
    }

    /**
     * Die Zelle {@code langpack/add/k700/n2000} der Rust-Suite (wie
     * {@link Langpack#add}): so viele verankerte Suchen wie Rust auf
     * fa12974, gezählt mit {@code cargo bench --bench suite --features
     * perf_trace -- run langpack/add/k700/n2000 --samples 1}
     * ({@code anchored_searches=4952}, {@code seed_searches=0},
     * {@code anchor_walk_visits=27799}). Schritte, Knoten und
     * Fingerprint wie dort.
     */
    @Test
    void verankerteSuchenWieRust() {
        Langpack.Spec spec = new Langpack.Spec(700, 2000);
        Langpack.Model m = new Langpack.Model(spec);
        List<Rule> rules = Rules.load(Langpack.rulesJson(spec.kinds()), m.g);
        Engine e = new Engine(rules, true);
        e.elementsAdded(m.g, m.vs, m.all);
        long steps = 0;
        while (e.step(m.g, m.vs) != null) steps++;
        assertEquals("5246 2623 12554 a4291bcc2e50900c", steps + " " + e.cascade.size() + " "
                + m.g.allNodes().size() + " " + Langpack.fingerprint(m.g, e, m.vs),
                "Schritte, angewandte Schritte, Knoten, Fingerprint");
        assertEquals("anchored_searches=4952 seed_searches=0 anchor_walk_visits=27799",
                "anchored_searches=" + e.anchoredSearches + " seed_searches=" + e.seedSearches
                        + " anchor_walk_visits=" + e.walk.visits);
    }

    /**
     * Regeln, die viele verschiedene Typen unter einen Typ hängen (Rust
     * {@code the_index_keeps_the_matches_under_many_groups}): der
     * Präfixbaum-Knoten von {@code Item} hat zwei Gruppen je Regel, eine
     * für das Blatt, nach dem die Regel fragt, eine für den Knoten, unter
     * dem das Item hängt. Items mit kurzer Beteiligungsliste, eines mit
     * einer Liste, die eine {@code TreeSet} ist und noch durchlaufen wird
     * (höchstens acht Einträge je Gruppe), und eines mit einer längeren
     * geben dieselben Matches über das Delta, den Seed über den Index und
     * den Scan.
     */
    @Test
    void vieleGruppenUnterEinemTyp() {
        final int kinds = 8;
        final int items = 10;
        // Einträge der mittleren Liste: eine TreeSet (mehr als 48) und
        // höchstens acht je Gruppe der sechzehn Gruppen.
        final int middle = 60;
        // Einträge der langen Liste: mehr als acht je Gruppe.
        final int longList = 2 * kinds * 8 + 20;
        StringBuilder json = new StringBuilder("{\"format\":3,\"name\":\"anchor_groups\",\"rules\":[");
        for (int k = 0; k < kinds; k++) {
            if (k > 0) json.append(',');
            json.append("""
                {"name":"R%1$d","rank":1,
                 "left":{"anchor":"item","nodes":[
                   {"name":"item","type":"Item"},
                   {"name":"attr","type":"Attr%1$d","context":true},
                   {"name":"over","type":"Over%1$d","context":true}],
                   "links":[["item","attr"],["over","item"]]},
                 "right":{"anchor":"marker","nodes":[{"name":"marker","type":"Marker%1$d"}],"links":[]},
                 "corrs":[{"type":"C%1$d","left":"item","right":"marker","role":"establishes"}]}"""
                    .formatted(k));
        }
        json.append("]}");
        for (int mode = 0; mode < 3; mode++) {
            Graph g = new Graph();
            List<Id> all = new ArrayList<>();
            for (int j = 0; j < items; j++) item(g, all, "item" + j, j % kinds, 1);
            item(g, all, "middle", 3, middle - 2);
            item(g, all, "long", 5, longList - 2);
            List<Rule> rules = Rules.load(json.toString(), g);
            Engine e = new Engine(rules, mode != 2);
            switch (mode) {
                case 0 -> e.elementsAdded(g, Map.of(), all);
                case 1 -> {
                    e.seedFactor = 0;
                    e.seed(g, Map.of());
                    assertEquals(1, e.seedsByIndex, "der Seed ging nicht über den Index");
                }
                default -> e.seed(g, Map.of());
            }
            while (e.step(g, Map.of()) != null) { }
            // Ein angewandter Schritt je Item.
            assertEquals(items + 2, e.cascade.size(), "Weg " + mode);
        }
    }

    /** Ein Item der Art {@code k} mit {@code others} Blättern, die keine
     *  Regel kennt. */
    private static void item(Graph g, List<Id> all, String name, int k, int others) {
        Id item = g.addBaseline(name, "Item");
        Id attr = g.addBaseline(name + "/a", "Attr" + k);
        Id over = g.addBaseline(name + "/up", "Over" + k);
        g.connect(item, attr, St.SOLID);
        g.connect(over, item, St.SOLID);
        all.addAll(List.of(item, attr, over));
        for (int n = 0; n < others; n++) {
            Id l = g.addBaseline(name + "/o" + n, "Other" + (n % 7));
            g.connect(item, l, St.SOLID);
            all.add(l);
        }
    }

    /** Tripel und Anfänge je Knoten für {@link AnchorIndex#inOrder}. */
    private record Triples(long[] out, int[] groups) {}

    /** Knoten mit bis zu vier Tripeln, ein leerer Knoten zuerst,
     *  dazwischen und zuletzt, {@code total} Tripel (Rust
     *  {@code switch_input}). */
    private static Triples switchInput(int total) {
        List<Long> out = new ArrayList<>();
        List<Integer> groups = new ArrayList<>(List.of(0));
        int ni = 1;
        while (out.size() < total) {
            groups.add(out.size());
            for (int pos = 3; pos >= 0; pos--) {
                if (out.size() < total) out.add(AnchorIndex.pack((ni * 7 + pos * 5) % 23, ni, pos));
            }
            ni++;
            if (ni % 9 == 0) {
                groups.add(out.size());
                ni++;
            }
        }
        groups.add(out.size());
        return new Triples(out.stream().mapToLong(Long::longValue).toArray(),
                groups.stream().mapToInt(Integer::intValue).toArray());
    }

    private static void sorts(Triples t, String what) {
        long[] expected = t.out().clone();
        java.util.Arrays.sort(expected);
        long[] out = t.out().clone();
        assertArrayEquals(expected, AnchorIndex.inOrder(out, out.length, t.groups(), t.groups().length),
                what);
    }

    /** Um die Länge, ab der die Tripel nach Regel verteilt statt als
     *  Ganzes sortiert werden (Rust {@code in_order_at_the_switch}). */
    @Test
    void inOrderAnDerUmschaltung() {
        assertEquals(256, AnchorIndex.WHOLE);
        for (int total : new int[] {0, 1, 255, 256, 257, 300}) {
            sorts(switchInput(total), total + " Tripel");
        }
    }

    /** Die Verteilung nach Regel ergibt die Folge einer Sortierung (Rust
     *  {@code in_order_is_a_sort}): bis zu 120 Knoten mit je bis zu elf
     *  verschiedenen (Regel, Position)-Paaren. */
    @Test
    void inOrderIstEineSortierung() {
        java.util.Random rnd = new java.util.Random(11);
        int distributed = 0;
        for (int c = 0; c < 256; c++) {
            List<Long> out = new ArrayList<>();
            List<Integer> groups = new ArrayList<>();
            int nodes = rnd.nextInt(120);
            for (int ni = 0; ni < nodes; ni++) {
                groups.add(out.size());
                java.util.TreeSet<Integer> seen = new java.util.TreeSet<>();
                int places = rnd.nextInt(12);
                for (int k = 0; k < places; k++) {
                    int rule = rnd.nextInt(40);
                    int pos = rnd.nextInt(6);
                    if (seen.add(rule * 6 + pos)) out.add(AnchorIndex.pack(rule, ni, pos));
                }
            }
            if (out.size() > AnchorIndex.WHOLE) distributed++;
            sorts(new Triples(out.stream().mapToLong(Long::longValue).toArray(),
                    groups.stream().mapToInt(Integer::intValue).toArray()), "Fall " + c);
        }
        assertTrue(distributed > 64, "nur " + distributed + " Fälle über der Umschaltung");
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
