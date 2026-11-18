package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.function.Consumer;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;

/**
 * Option E (Rust 04394d9): "angewandt" ist eine Marke an der Nummer des
 * Matches, die der Kandidat im Todo traegt ({@link Applied}). Eine Engine
 * mit Pruefung fuehrt daneben die Menge der angewandten Schluessel wie
 * 2.0.2 ({@link Applied#check}) und vergleicht bei jeder Frage.
 *
 * <p>Hier wird eine solche Engine durch Zufallsszenarien mit
 * Host-Loeschungen, geloeschten Erzeugnissen, Wertaenderungen und neuen
 * Knoten gefahren. Vor dem ersten Schritt jeder Welle, nach jedem
 * Schritt und nach jeder Konsolidierung muss gelten:
 * <ul>
 * <li>die Schluessel der markierten Matches sind die Menge, jeder einmal,
 * <li>jeder markierte Match steht unter seinem Schluessel in
 *     {@code byKey}, und jeder Kandidat im Todo traegt den Match, der
 *     unter seinem Schluessel steht (die Grundlage der Marke),
 * <li>ein Schluessel, dessen Marke vergessen und der wieder angewandt
 *     wurde, ist an einem anderen Match markiert als vorher.
 * </ul>
 * Die Abzuege samt Schrittfolge sind die einer Engine ohne diese
 * Pruefung.
 */
final class AppliedTest {

    /** Was alle Laeufe einer Welt zusammen durchlaufen haben. */
    private static final class Totals {
        int states, withMarks, forgotten, again, orphaned, applied;
    }

    /** Prueft eine Engine in jedem Zustand, den {@link Worlds#run} ihr
     *  zeigt. Eine Instanz je Lauf. */
    private static final class Watch implements Consumer<Engine> {
        final String where;
        final Totals totals;
        /** Die im vorigen Zustand markierten Schluessel mit der Nummer
         *  ihres Matches. */
        Map<MatchKey, Integer> before = Map.of();
        /** Die Schluessel, deren Marke vergessen wurde, mit der Nummer
         *  des Matches, an dem sie stand. */
        final Map<MatchKey, Integer> forgotten = new HashMap<>();

        Watch(String where, Totals totals) {
            this.where = where;
            this.totals = totals;
        }

        @Override public void accept(Engine e) {
            String at = where + ", Zustand " + totals.states;
            totals.states++;
            Map<MatchKey, Integer> marked = new HashMap<>();
            for (int mix = 0; mix < e.matches.size(); mix++) {
                if (!e.applied.marked(mix)) continue;
                MatchKey k = e.matches.get(mix).key;
                assertNull(marked.put(k, mix), at + ": ein Schluessel an zwei Matches markiert");
                assertEquals(Integer.valueOf(mix), e.byKey.get(k),
                        at + ": markierter Match steht nicht unter seinem Schluessel");
            }
            assertEquals(e.applied.check, marked.keySet(), at + ": Marken und Menge der angewandten Schluessel");
            List<MatchKey> keys = e.appliedKeys();
            assertEquals(marked.size(), keys.size(), at + ": appliedKeys");
            assertEquals(marked.keySet(), new HashSet<>(keys), at + ": appliedKeys");
            assertEquals(marked.isEmpty(), e.applied.isEmpty(), at + ": isEmpty");
            for (Engine.TodoKey t : e.todo) {
                assertEquals(Integer.valueOf(t.mix), e.byKey.get(t.key),
                        at + ": der Kandidat traegt nicht den Match unter seinem Schluessel");
                assertSame(e.matches.get(t.mix).key, t.key, at + ": Schluessel des Kandidaten");
            }
            // Ein lebender Match, der nicht unter seinem Schluessel steht:
            // sein Kandidat wurde mit tentativ zurueckgezogenem Anker
            // gezogen und vergessen.
            for (int mix = 0; mix < e.matches.size(); mix++) {
                Engine.MatchRec m = e.matches.get(mix);
                Integer filed = e.byKey.get(m.key);
                if (!m.dead && (filed == null || filed != mix)) {
                    totals.orphaned++;
                    break;
                }
            }
            for (Map.Entry<MatchKey, Integer> b : before.entrySet()) {
                if (!marked.containsKey(b.getKey())) {
                    totals.forgotten++;
                    forgotten.put(b.getKey(), b.getValue());
                }
            }
            for (Map.Entry<MatchKey, Integer> m : marked.entrySet()) {
                Integer old = forgotten.remove(m.getKey());
                if (old != null) {
                    totals.again++;
                    assertNotEquals(old, m.getValue(), at + ": wieder angewandt am alten Match");
                }
            }
            if (!marked.isEmpty()) totals.withMarks++;
            before = marked;
        }
    }

    private static int applied(List<String> dumps) {
        int n = 0;
        for (String d : dumps) {
            int at = d.indexOf("steps=");
            int end = d.indexOf('\n', at);
            for (int i = at + 6; i < end; i++) if (d.charAt(i) == 'a') n++;
        }
        return n;
    }

    private static Totals world(Worlds.World world, boolean routed, int maxWaves, int cases) {
        Totals totals = new Totals();
        boolean useIndex = Engine.defaultUseIndex();
        for (long seed = 1; seed <= cases; seed++) {
            Random rnd = new Random(seed * 0x9E3779B97F4A7C15L + world.name().hashCode());
            List<Worlds.Wave> waves = Worlds.scenario(rnd, maxWaves);
            String where = world.name() + (routed ? "/gerichtet" : "/ungerichtet") + " seed " + seed;
            List<String> checked = Worlds.run(world, routed, waves,
                    rules -> new Engine(rules, useIndex, true), false, new Watch(where, totals));
            List<String> plain = Worlds.run(world, routed, waves, useIndex, false);
            assertEquals(plain, checked, where + ": Abzuege mit und ohne Pruefung");
            totals.applied += applied(checked);
        }
        System.out.println("AppliedTest " + world.name() + (routed ? "/gerichtet" : "/ungerichtet")
                + " zustaende=" + totals.states + " mit_marken=" + totals.withMarks
                + " vergessen=" + totals.forgotten + " wieder_angewandt=" + totals.again
                + " vergessen_tentativ=" + totals.orphaned + " angewandt=" + totals.applied);
        return totals;
    }

    /** Beide Richtungen. Wieder angewandt wird in diesen Szenarien kein
     *  Schluessel (gemessen 0), das zeigt {@link #familyGerichtetMarkeWieMenge}. */
    @Test
    void followupMarkeWieMenge() {
        for (boolean routed : new boolean[] {true, false}) {
            Totals t = world(Worlds.followup(), routed, 14, 300);
            assertTrue(t.applied > 300, "angewandt " + t.applied);
            assertTrue(t.forgotten > 0, "vergessen " + t.forgotten);
        }
    }

    /** Die lebendigen Korrespondenzen an {@code x}. */
    private static int liveCorrsAt(Graph g, Id x) {
        int n = 0;
        for (Part p : g.parts(x)) {
            Node o = g.node(p.other);
            if (o != null && o.isCorr && o.status.alive()) n++;
        }
        return n;
    }

    /** Eine Engine fuer eine Welle von der Quellseite, mit oder ohne
     *  Pruefung. */
    private static Engine forward(LinksTest.ContextLink w, boolean checks) {
        Engine e = new Engine(w.rules(), Engine.defaultUseIndex(), checks);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        return e;
    }

    /** Die Nummer des einen Matches einer Regel {@code Child}. */
    private static int child(Engine e) {
        int found = -1;
        for (int mix = 0; mix < e.matches.size(); mix++) {
            if (e.rules.get(e.matches.get(mix).ruleIx).name.startsWith("Child")) {
                assertEquals(-1, found, "ein Match von Child");
                found = mix;
            }
        }
        assertTrue(found >= 0, "ein Match von Child");
        return found;
    }

    /**
     * Ein Host ruft das oeffentliche {@link Engine#retractMatch} fuer einen
     * toten Match, dessen Schluessel inzwischen ein anderer Match traegt.
     * 2.0.2 fuehrte Todo, angewandte Schluessel und Provenienz ueber den
     * Schluessel: der Aufruf nahm ihn dem anderen Match, der Kandidat im Todo
     * blieb, und wurde er angewandt, bekam kein Match die Provenienzkante.
     * Derselbe Ablauf ueber die oeffentliche API gegen fa12974 gefahren
     * (Job 7398), die erwarteten Werte sind die von dort. Drei Lagen:
     * <ul>
     * <li>der Kandidat des anderen steht noch im Todo (fall2),
     * <li>der andere ist schon angewandt (fall2b),
     * <li>der tote war angewandt, der andere holt dessen Erzeugnisse vor der
     *     Konsolidierung zurueck (fall2c).
     * </ul>
     * Mit und ohne Pruefung.
     */
    @Test
    void einToterMatchNimmtDemAnderenDenSchluesselWie202() {
        for (boolean checks : new boolean[] {false, true}) {
            String at = checks ? "mit Pruefung" : "ohne Pruefung";
            // fall2: der Kandidat des anderen steht noch im Todo.
            LinksTest.ContextLink w = LinksTest.contextLinkGraph(1);
            Graph g = w.g();
            Id x0 = w.xs().get(0);
            Map<Id, String> vals = new HashMap<>();
            Engine e = forward(w, checks);
            e.seed(g, vals);
            assertEquals(Boolean.TRUE, e.step(g, vals), at + ": Root");
            int dead = child(e);
            int rule = e.matches.get(dead).ruleIx;
            Id[] refs = e.matches.get(dead).refs.clone();
            e.elementRemoved(x0);
            e.retractMatch(g, dead, new ArrayList<>());
            e.record(rule, refs.clone());
            int other = e.matches.size() - 1;
            ArrayList<Id> queue = new ArrayList<>();
            e.retractMatch(g, dead, queue);
            assertEquals(0, queue.size(), at);
            e.drainRetraction(g, queue);
            assertEquals(Boolean.TRUE, e.step(g, vals), at + ": der Kandidat des anderen wird angewandt");
            assertNull(e.matches.get(other).entry, at + ": ohne Provenienzkante");
            assertEquals(1, liveCorrsAt(g, x0), at);
            e.record(rule, refs.clone());
            assertEquals(4, e.matches.size(), at + ": der Schluessel war frei");
            assertNull(e.step(g, vals), at + ": der Schluessel ist angewandt, der dritte wird uebersprungen");
            assertEquals(2, e.cascade.size(), at);
            assertEquals(Engine.Termination.DUPLICATION, e.verdict(), at);
            g.setNodeStatus(x0, St.TOMBSTONE);
            e.elementDeleted(g, x0);
            assertEquals(0, e.consolidate(g), at + ": x0 geloescht");
            assertEquals(1, liveCorrsAt(g, x0), at + ": das Erzeugnis des anderen bleibt wie in 2.0.2");
            g.setNodeStatus(w.a(), St.TOMBSTONE);
            e.elementDeleted(g, w.a());
            assertEquals(4, e.consolidate(g), at + ": a geloescht");
            assertEquals(Engine.Termination.CONVERGENCE, e.verdict(), at + ": nichts mehr angewandt");

            // fall2b: der andere ist schon angewandt.
            w = LinksTest.contextLinkGraph(1);
            g = w.g();
            x0 = w.xs().get(0);
            e = forward(w, checks);
            e.seed(g, vals);
            assertEquals(Boolean.TRUE, e.step(g, vals), at + ": Root");
            dead = child(e);
            rule = e.matches.get(dead).ruleIx;
            refs = e.matches.get(dead).refs.clone();
            e.elementRemoved(x0);
            e.retractMatch(g, dead, new ArrayList<>());
            e.record(rule, refs.clone());
            other = e.matches.size() - 1;
            assertEquals(Boolean.TRUE, e.step(g, vals), at + ": der andere");
            assertNull(e.step(g, vals), at);
            assertEquals(Integer.valueOf(1), e.matches.get(other).entry, at);
            queue = new ArrayList<>();
            e.retractMatch(g, dead, queue);
            assertEquals(0, queue.size(), at);
            e.drainRetraction(g, queue);
            e.record(rule, refs.clone());
            assertEquals(Boolean.FALSE, e.step(g, vals), at + ": der dritte ist ein Duplikat");
            assertNull(e.step(g, vals), at);
            assertEquals(0, e.consolidate(g), at);
            g.setNodeStatus(x0, St.TOMBSTONE);
            e.elementDeleted(g, x0);
            assertEquals(5, e.consolidate(g), at + ": x0 geloescht");
            assertEquals(0, liveCorrsAt(g, x0), at);

            // fall2c: der tote war angewandt, der andere holt die Erzeugnisse zurueck.
            w = LinksTest.contextLinkGraph(1);
            g = w.g();
            x0 = w.xs().get(0);
            e = forward(w, checks);
            e.seed(g, vals);
            LinksTest.runRemaining(e, g, vals);
            assertEquals(2, e.cascade.size(), at + ": Root und Child");
            dead = child(e);
            rule = e.matches.get(dead).ruleIx;
            refs = e.matches.get(dead).refs.clone();
            queue = new ArrayList<>();
            e.retractMatch(g, dead, queue);
            e.drainRetraction(g, queue);
            e.record(rule, refs.clone());
            other = e.matches.size() - 1;
            queue = new ArrayList<>();
            e.retractMatch(g, dead, queue);
            assertEquals(2, queue.size(), at + ": die Erzeugnisse des toten noch einmal");
            e.drainRetraction(g, queue);
            assertEquals(Boolean.TRUE, e.step(g, vals), at + ": der andere");
            assertNull(e.step(g, vals), at);
            assertEquals(0, e.consolidate(g), at);
            assertEquals(3, e.cascade.size(), at);
            assertNull(e.matches.get(other).entry, at + ": ohne Provenienzkante");
            g.setNodeStatus(x0, St.TOMBSTONE);
            e.elementDeleted(g, x0);
            assertEquals(5, e.consolidate(g), at + ": x0 geloescht");
            assertEquals(0, liveCorrsAt(g, x0), at);
        }
    }

    /** Gerichtet, ungerichtet saettigt die Welt nicht. Hier werden
     *  Schluessel vergessen und wieder angewandt, und Kandidaten mit
     *  tentativ zurueckgezogenem Anker vergessen ihren Match (gemessen
     *  12036, 23 und 6009 Zustaende). */
    @Test
    void familyGerichtetMarkeWieMenge() {
        Totals t = world(Worlds.family(), true, 24, 200);
        assertTrue(t.applied > 200, "angewandt " + t.applied);
        assertTrue(t.forgotten > 0, "vergessen " + t.forgotten);
        assertTrue(t.again > 0, "wieder angewandt " + t.again);
        assertTrue(t.orphaned > 0, "vergessen mit tentativem Anker " + t.orphaned);
    }
}
