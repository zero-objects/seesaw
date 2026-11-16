package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashSet;
import java.util.List;
import java.util.Random;
import java.util.Set;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;

/**
 * Reihenfolge der Todo-Entnahme: gleich der Ordnung von
 * {@code TodoKey} in 2.0.2 (Rang absteigend, Ref-Folge absteigend,
 * Regelindex aufsteigend), geprueft an Zufallsschluesseln, deren erste
 * Ids oft einen gemeinsamen Anfang haben.
 */
final class TodoKeyTest {

    /** Laenge des gemeinsamen Anfangs zweier Ids, auch genau an den
     *  Grenzen von 8 und 16 Byte. */
    private static final int[] SHARE = {0, 1, 7, 8, 9, 15, 16, 17, 24, 31, 32};

    /**
     * n Ids; etwa zwei Drittel haben mit einer frueheren Id einen
     * gemeinsamen Anfang (0 bis 32 Byte), 32 Byte sind eine gleiche Id
     * in einem anderen Objekt.
     */
    static Id[] pool(Random rnd, int n) {
        Id[] p = new Id[n];
        for (int i = 0; i < n; i++) {
            byte[] b = new byte[32];
            rnd.nextBytes(b);
            if (i > 0 && rnd.nextInt(3) > 0) {
                int share = SHARE[rnd.nextInt(SHARE.length)];
                System.arraycopy(p[rnd.nextInt(i)].b, 0, b, 0, share);
            }
            p[i] = new Id(b);
        }
        return p;
    }

    /** Eine Ref-Folge der Laenge 0 bis maxLen aus dem Vorrat. */
    static Id[] refs(Random rnd, Id[] pool, int maxLen) {
        Id[] r = new Id[rnd.nextInt(maxLen + 1)];
        for (int i = 0; i < r.length; i++) r[i] = pool[rnd.nextInt(pool.length)];
        return r;
    }

    /** Regeln ohne Muster und ohne Erzeugnis, nur mit Rang. */
    static List<Rule> rules(long... ranks) {
        List<Rule> rules = new ArrayList<>();
        for (int i = 0; i < ranks.length; i++) {
            Rule r = new Rule();
            r.name = "r" + i;
            r.rank = ranks[i];
            r.patNodes = List.of();
            r.patLinks = List.of();
            r.createNodes = List.of();
            r.createLinks = List.of();
            r.inputTypes = List.of();
            rules.add(r);
        }
        return rules;
    }

    /** Ein Todo-Eintrag, wie 2.0.2 ihn verglich. */
    private record Alt(long rank, Id[] refs, int ruleIx) {}

    /** {@code Id.compareTo} von 2.0.2: Byte fuer Byte ohne Vorzeichen. */
    private static int altId(Id a, Id b) {
        for (int i = 0; i < 32; i++) {
            int x = a.b[i] & 0xFF, y = b.b[i] & 0xFF;
            if (x != y) return Integer.compare(x, y);
        }
        return 0;
    }

    /** {@code Matcher.compareBindings} von 2.0.2. */
    private static int altFolge(Id[] a, Id[] b) {
        int n = Math.min(a.length, b.length);
        for (int i = 0; i < n; i++) {
            int c = altId(a[i], b[i]);
            if (c != 0) return c;
        }
        return Integer.compare(a.length, b.length);
    }

    /** {@code TodoKey.compareTo} von 2.0.2: (Reverse(rank), Reverse(refs), ruleIx). */
    private static int alt(long rankA, Id[] refsA, int ruleA, long rankB, Id[] refsB, int ruleB) {
        int c = Long.compare(rankB, rankA);
        if (c != 0) return c;
        c = altFolge(refsB, refsA);
        if (c != 0) return c;
        return Integer.compare(ruleA, ruleB);
    }

    private static int alt(Alt a, Alt b) {
        return alt(a.rank, a.refs, a.ruleIx, b.rank, b.refs, b.ruleIx);
    }

    private static final long[] RANKS = {5, 900, 5, 0, 1, 5, 1L << 40};

    private static void gleicheFolge(String wo, List<Alt> erwartet, List<Engine.TodoKey> ist) {
        assertEquals(erwartet.size(), ist.size(), wo + ": Laenge");
        for (int i = 0; i < erwartet.size(); i++) {
            Alt a = erwartet.get(i);
            Engine.TodoKey t = ist.get(i);
            String at = wo + ": Stelle " + i;
            assertEquals(a.rank, t.rank, at);
            assertEquals(a.ruleIx, t.ruleIx, at);
            assertArrayEquals(a.refs, t.refs, at);
        }
    }

    @Test
    void todoReihenfolgeWieDieAlteTodoKeyOrdnung() {
        for (int seed = 1; seed <= 5; seed++) {
            Random rnd = new Random(seed);
            Id[] pool = pool(rnd, 40);
            Engine e = new Engine(rules(RANKS), false);
            List<Alt> erwartet = new ArrayList<>();
            Set<String> gesehen = new HashSet<>();
            for (int i = 0; i < 1000; i++) {
                int ruleIx = rnd.nextInt(RANKS.length);
                Id[] refs = refs(rnd, pool, 3);
                e.record(ruleIx, refs);
                if (gesehen.add(Engine.key(ruleIx, refs))) erwartet.add(new Alt(RANKS[ruleIx], refs, ruleIx));
            }
            erwartet.sort(TodoKeyTest::alt);
            String wo = "seed " + seed;
            assertTrue(erwartet.size() > 300, wo + ": nur " + erwartet.size() + " Schluessel");
            List<Engine.TodoKey> ist = new ArrayList<>(e.todo);
            gleicheFolge(wo, erwartet, ist);

            // Jedes Paar vergleicht wie in 2.0.2, auch gegen eine Schranke
            // wie in stepWithLimit (Regelindex 0, nicht in der Menge).
            for (Engine.TodoKey a : ist) {
                for (Engine.TodoKey b : ist) {
                    assertEquals(Integer.signum(alt(a.rank, a.refs, a.ruleIx, b.rank, b.refs, b.ruleIx)),
                            Integer.signum(a.compareTo(b)), wo);
                }
            }
            for (int i = 0; i < 200; i++) {
                long rank = RANKS[rnd.nextInt(RANKS.length)];
                Id[] refs = refs(rnd, pool, 3);
                Engine.TodoKey bound = new Engine.TodoKey(rank, refs, 0);
                for (Engine.TodoKey b : ist) {
                    assertEquals(Integer.signum(alt(rank, refs, 0, b.rank, b.refs, b.ruleIx)),
                            Integer.signum(bound.compareTo(b)), wo + ": Schranke");
                }
            }

            // Matches eines entfernten Elements verlassen das Todo.
            for (int i = 0; i < 6; i++) {
                Id weg = pool[rnd.nextInt(pool.length)];
                e.elementRemoved(weg);
                erwartet.removeIf(a -> Arrays.asList(a.refs).contains(weg));
            }
            gleicheFolge(wo + " nach elementRemoved", erwartet, new ArrayList<>(e.todo));

            // Entnahme wie in step: immer das erste.
            List<Engine.TodoKey> entnommen = new ArrayList<>();
            while (!e.todo.isEmpty()) entnommen.add(e.todo.pollFirst());
            gleicheFolge(wo + " Entnahme", erwartet, entnommen);
        }
    }
}
