package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
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
