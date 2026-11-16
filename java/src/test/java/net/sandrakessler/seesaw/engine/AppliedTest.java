package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.function.Consumer;

import org.junit.jupiter.api.Test;

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
