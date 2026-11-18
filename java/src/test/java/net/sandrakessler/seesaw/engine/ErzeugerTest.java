package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Die Erzeuger eines Elements ({@link Engine#producersOf}). Ein Knoten
 * steht in {@code byProduct}, die Erzeuger einer Verbindung kommen aus
 * ihren Enden. 2.0.2 trug je Schritt jedes Erzeugnis ein, Knoten und
 * Verbindungen. Verglichen wird mit dieser Ablage, aus der Kaskade
 * nachgerechnet.
 */
final class ErzeugerTest {
    private static final String[] NAMES = {"Ada", "Bob", "Cy"};

    /**
     * Was 2.0.2 fuer {@code id} in {@code byProduct} abgelegt haette:
     * je Nennung in {@code created} oder {@code createdEdges} eines
     * Eintrags dessen Index, in der Folge der Kaskade.
     */
    static int[] abgelegt(Engine e, Id id) {
        ArrayList<Integer> out = new ArrayList<>();
        for (int eix = 0; eix < e.cascade.size(); eix++) {
            Engine.Entry en = e.cascade.get(eix);
            for (Id c : en.created) if (c.equals(id)) out.add(eix);
            for (Id c : en.createdEdges) if (c.equals(id)) out.add(eix);
        }
        int[] a = new int[out.size()];
        for (int i = 0; i < a.length; i++) a[i] = out.get(i);
        return a;
    }

    /**
     * Ohne aufeinanderfolgende Wiederholungen. Ein Eintrag, der eine
     * Verbindung zweimal nennt, stand in 2.0.2 zweimal in der Liste.
     * Der zweite {@code retractEntry} fand den Schluessel nicht mehr in
     * {@code byKey} (der erste hatte ihn entfernt) und tat nichts.
     */
    static int[] ohneWiederholung(int[] a) {
        int n = 0;
        int[] out = new int[a.length];
        for (int i = 0; i < a.length; i++) {
            if (n == 0 || out[n - 1] != a[i]) out[n++] = a[i];
        }
        return Arrays.copyOf(out, n);
    }

    private static int[] liste(IntList l) {
        return l == null ? new int[0] : l.toArray();
    }

    /** Schritte bis zur Saettigung, hoechstens 2000 (wie
     *  {@link Worlds#steps}). */
    private static void saettigen(Engine e, Graph g, Map<Id, String> vals) {
        for (int i = 0; i < 2000; i++) {
            if (e.step(g, vals) == null) return;
        }
        throw new AssertionError("keine Saettigung nach 2000 Schritten");
    }

    /**
     * Welt {@code family}: drei Familien, je zwei Vaeter mit je zwei
     * Mitgliedern. Das erste Mitglied steht zusaetzlich unter jedem
     * spaeteren Vater, seine Erzeugnisse haben also mehrere Erzeuger.
     * Die Welle ist gerichtet, von der Quellseite: ungerichtet saettigt
     * die Welt nicht (nach 20000 Schritten 6672 Eintraege). Nach der
     * Saettigung wird jede erzeugte Verbindung nacheinander
     * geloescht (Tombstone, {@code elementRemoved},
     * {@code elementDeleted}, Seed, Schritte, Konsolidierung). Vor jeder
     * Loeschung muessen die Erzeuger jeder Verbindung und jedes Knotens
     * die von 2.0.2 sein.
     */
    @Test
    void erzeugteVerbindungenNacheinanderGeloeschtWieVorher() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(Worlds.family().rules(), g);
        Map<Id, String> vals = new HashMap<>();
        Id shared = null;
        for (int f = 0; f < 3; f++) {
            Id fam = g.addBaseline("fam" + f, "Family");
            for (int v = 0; v < 2; v++) {
                Id father = g.addBaseline("father" + f + v, "Father");
                g.connect(fam, father, St.SOLID);
                if (shared != null) g.connect(father, shared, St.SOLID);
                for (int m = 0; m < 2; m++) {
                    Id member = g.addBaseline("member" + f + v + m, "Member");
                    g.connect(father, member, St.SOLID);
                    Id first = g.addBaseline("first" + f + v + m, "firstName");
                    vals.put(first, NAMES[(f + v + m) % NAMES.length]);
                    g.connect(member, first, St.SOLID);
                    if (shared == null) shared = member;
                }
            }
        }
        Engine e = new Engine(rules);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, vals);
        saettigen(e, g, vals);
        e.consolidate(g);

        int runden = 0;
        int verbindungenMitErzeuger = 0;
        int knotenMitMehrerenErzeugern = 0;
        while (true) {
            List<Conn> conns = g.allConnections();
            conns.sort((x, y) -> x.id.compareTo(y.id));
            Id next = null;
            for (Conn c : conns) {
                int[] want = ohneWiederholung(abgelegt(e, c.id));
                assertArrayEquals(want, liste(e.producersOf(g, c.id)),
                        "Runde " + runden + ", Verbindung " + Worlds.hex(c.id));
                if (want.length > 0) {
                    verbindungenMitErzeuger++;
                    if (next == null && c.status.matchable()) next = c.id;
                }
            }
            for (Node n : g.allNodes()) {
                int[] want = abgelegt(e, n.id);
                assertArrayEquals(want, liste(e.producersOf(g, n.id)),
                        "Runde " + runden + ", Knoten " + Worlds.hex(n.id));
                if (want.length > 1) knotenMitMehrerenErzeugern++;
            }
            if (next == null) break;
            g.setConnectionStatus(next, St.TOMBSTONE);
            e.elementRemoved(next);
            e.elementDeleted(g, next);
            e.seed(g, vals);
            saettigen(e, g, vals);
            e.consolidate(g);
            runden++;
        }
        assertTrue(runden >= 10, "Runden: " + runden);
        assertTrue(verbindungenMitErzeuger >= 100, "Verbindungen mit Erzeuger: " + verbindungenMitErzeuger);
        assertTrue(knotenMitMehrerenErzeugern > 0, "Knoten mit mehreren Erzeugern: " + knotenMitMehrerenErzeugern);
    }

    /** Die Regeln von {@code LinksTest} und {@code Pair}: zwei
     *  {@code X} mit einer Verbindung, deren {@code Y} ueber ihre
     *  Korrespondenzen gematcht werden. Die Verbindung zwischen den beiden
     *  {@code Y} erzeugt {@code Pair} zwischen zwei gematchten Knoten. */
    private static final String PAIR_RULES = """
            {
              "format": 3,
              "name": "context_link_pair",
              "rules": [
                {
                  "name": "Root", "rank": 100,
                  "left": {"anchor": "a", "nodes": [{"name": "a", "type": "A"}]},
                  "right": {"anchor": "b", "nodes": [{"name": "b", "type": "B"}]},
                  "corrs": [{"type": "AB", "left": "a", "right": "b", "role": "establishes"}]
                },
                {
                  "name": "Child", "rank": 90,
                  "left": {"anchor": "a", "nodes": [
                    {"name": "a", "type": "A"}, {"name": "x", "type": "X"}
                  ], "links": [["a", "x"]]},
                  "right": {"anchor": "b", "nodes": [
                    {"name": "b", "type": "B"}, {"name": "y", "type": "Y"}
                  ], "links": [["b", "y"]]},
                  "corrs": [
                    {"type": "AB", "left": "a", "right": "b", "role": "references"},
                    {"type": "XY", "left": "x", "right": "y", "role": "establishes"}
                  ]
                },
                {
                  "name": "Pair", "rank": 80,
                  "left": {"anchor": "x1", "nodes": [
                    {"name": "x1", "type": "X"}, {"name": "x2", "type": "X"}
                  ], "links": [["x1", "x2"]]},
                  "right": {"anchor": "y1", "nodes": [
                    {"name": "y1", "type": "Y"}, {"name": "y2", "type": "Y"}
                  ], "links": [["y1", "y2"]]},
                  "corrs": [
                    {"type": "XY", "left": "x1", "right": "y1", "role": "references"},
                    {"type": "XY", "left": "x2", "right": "y2", "role": "references"}
                  ]
                }
              ]
            }
            """;

    /**
     * Ein Eintrag legt eine Verbindung zwischen zwei gematchten Knoten an
     * ({@code Pair}: {@code y0 → y1}). Ablauf wie AppliedTest, Lage fall2:
     * der Match von {@code Pair} stirbt und verliert seinen Schluessel, ein
     * anderer Match bekommt ihn, ein Host ruft {@link Engine#retractMatch}
     * fuer den toten, der Kandidat des anderen wird angewandt und legt die
     * Verbindung an. Ein dritter Match T kommt unter den Schluessel und
     * uebernimmt die Marke. Dann loescht der Host die Verbindung. Die
     * Erzeuger einer Verbindung kommen aus ihren Enden: aus den Erzeugern
     * der Enden und den Eintraegen der Matches, die sie binden. Keines der
     * Enden hat der Eintrag erzeugt, ihn findet nur die Provenienzkante am
     * anderen Match. Gegen fa12974 gefahren (Job 7649): T stirbt, der
     * Schluessel ist nicht mehr angewandt, ein neuer Match unter ihm wird
     * angewandt. Ohne die Kante (a6102c4 bis fc0bdc4) blieb T lebendig,
     * mit {@code -Dseesaw.checks=true} warf {@code checkProducers}.
     */
    @Test
    void eineVerbindungZwischenGematchtenKnotenFindetIhrenErzeuger() {
        Graph g = new Graph();
        Id a = g.addBaseline("a", "A");
        Id x0 = g.addBaseline("a/x0", "X");
        Id x1 = g.addBaseline("a/x1", "X");
        g.connect(a, x0, St.SOLID);
        g.connect(a, x1, St.SOLID);
        g.connect(x0, x1, St.SOLID);
        List<Rule> rules = Rules.load(PAIR_RULES, g);
        Map<Id, String> vals = new HashMap<>();
        Engine e = LinksTest.forward(rules);
        e.seed(g, vals);
        for (int i = 0; i < 3; i++) assertEquals(Boolean.TRUE, e.step(g, vals), "Root und zwei Child");
        int pair = -1;
        for (int mix = 0; mix < e.matches.size(); mix++) {
            if (e.rules.get(e.matches.get(mix).ruleIx).name.startsWith("Pair")) pair = mix;
        }
        assertTrue(pair >= 0, "der Match von Pair");
        int rule = e.matches.get(pair).ruleIx;
        Id[] refs = e.matches.get(pair).refs.clone();
        e.elementRemoved(x1);
        e.retractMatch(g, pair, new ArrayList<>());
        e.record(rule, refs.clone());
        int other = e.matches.size() - 1;
        ArrayList<Id> queue = new ArrayList<>();
        e.retractMatch(g, pair, queue);
        e.drainRetraction(g, queue);
        assertEquals(Boolean.TRUE, e.step(g, vals), "der Kandidat des anderen");
        int eix = e.cascade.size() - 1;
        assertEquals(1, e.cascade.get(eix).createdEdges.size(), "y0 → y1");
        Id edge = e.cascade.get(eix).createdEdges.get(0);
        e.record(rule, refs.clone());
        int third = e.matches.size() - 1;
        assertNull(e.step(g, vals), "T wird uebersprungen");
        g.setConnectionStatus(edge, St.TOMBSTONE);
        e.elementDeleted(g, edge);
        assertTrue(e.matches.get(third).dead, "T ist tot");
        assertFalse(e.matches.get(other).dead, "der andere lebt");
        assertFalse(e.applied.marked(third), "der Schluessel ist nicht angewandt");
        assertFalse(e.appliedKeys().contains(e.matches.get(third).key));
        assertArrayEquals(new int[] {eix}, liste(e.producersOf(g, edge)), "der Eintrag des anderen");
        assertEquals(0, e.consolidate(g));
        e.record(rule, refs.clone());
        assertEquals(Boolean.TRUE, e.step(g, vals), "ein neuer Match unter dem Schluessel");
        assertNull(e.step(g, vals));
        assertEquals(5, e.cascade.size());
    }

    /**
     * Der Host macht mit der materialisierten Kopie weiter. Sie laesst eine
     * Verbindung weg, die {@code Root} erzeugt hat und die der Host vorher
     * zum Tombstone gemacht hat, und der Host gibt deren Id an eine Tuer.
     * 2.0.2 fand den Erzeuger in {@code byProduct}, zog den Match von
     * {@code Root} zurueck und mit ihm alles, was auf seiner Korrespondenz
     * aufbaut. Derselbe Ablauf ueber die oeffentliche API gegen fa12974
     * gefahren (Job 7397): nach der Tuer sind alle drei Matches tot, die
     * Konsolidierung macht 13 Elemente zu Tombstones, das Verdikt ist
     * {@code CONVERGENCE}. Je Tuer und je Verbindung von {@code Root} eine
     * frische Welt.
     */
    @Test
    @SuppressWarnings("deprecation")
    void eineVerbindungDieDieKopieWeglaesstFindetIhrenErzeuger() {
        for (int door = 0; door < 3; door++) {
            for (int pick = 0; pick < 2; pick++) {
                String at = "Tuer " + door + ", Verbindung " + pick;
                LinksTest.ContextLink w = LinksTest.contextLinkGraph(2);
                Graph g = w.g();
                Map<Id, String> vals = new HashMap<>();
                Engine e = LinksTest.forward(w.rules());
                e.seed(g, vals);
                saettigen(e, g, vals);
                e.consolidate(g);
                assertEquals(3, e.cascade.size(), at + ": Root und zwei Child");
                assertEquals(2, e.cascade.get(0).createdEdges.size(), at + ": die Verbindungen von Root");
                Id edge = e.cascade.get(0).createdEdges.get(pick);
                g.setConnectionStatus(edge, St.TOMBSTONE);
                Graph copy = g.materialize();
                assertTrue(copy.conn(edge) == null && copy.node(edge) == null, at + ": die Kopie haelt sie nicht");
                switch (door) {
                    case 0 -> e.elementDeleted(copy, edge);
                    case 1 -> e.elementChanged(copy, edge);
                    default -> e.retractFor(copy, edge);
                }
                for (int mix = 0; mix < e.matches.size(); mix++) {
                    assertTrue(e.matches.get(mix).dead, at + ": Match " + mix + " zurueckgezogen");
                }
                assertEquals(13, e.consolidate(copy), at + ": Tombstones");
                for (Engine.Entry en : e.cascade) {
                    for (Id c : en.created) assertEquals(St.TOMBSTONE, copy.node(c).status, at);
                }
                assertEquals(Engine.Termination.CONVERGENCE, e.verdict(), at);
                assertArrayEquals(new int[] {0}, liste(e.producersOf(copy, edge)), at + ": der Eintrag von Root");
            }
        }
    }
}
