package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.HashSet;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.Set;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

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

    /**
     * Gegenstueck zu {@code head_is_consistent_with_the_sequence_order}
     * auf der Rust-Seite: der Kopf sind die ersten 16 Byte der ersten Id
     * (0 fuer eine leere Folge), und wo sich zwei Koepfe unterscheiden,
     * ordnen sie wie die Folgen.
     */
    @Test
    void kopfOrdnetWieDieFolge() {
        Random rnd = new Random(13);
        Id[] pool = pool(rnd, 60);
        int verschieden = 0;
        int gleich = 0;
        for (int i = 0; i < 20_000; i++) {
            Id[] a = refs(rnd, pool, 2);
            Id[] b = refs(rnd, pool, 2);
            Engine.TodoKey x = new Engine.TodoKey(0, a, 0);
            Engine.TodoKey y = new Engine.TodoKey(0, b, 0);
            assertEquals(a.length == 0 ? 0L : ByteBuffer.wrap(a[0].b).getLong(0), x.head0);
            assertEquals(a.length == 0 ? 0L : ByteBuffer.wrap(a[0].b).getLong(8), x.head1);
            int c = Long.compareUnsigned(x.head0, y.head0);
            if (c == 0) c = Long.compareUnsigned(x.head1, y.head1);
            if (c == 0) {
                gleich++;
            } else {
                verschieden++;
                assertEquals(Integer.signum(altFolge(a, b)), Integer.signum(c));
            }
        }
        assertTrue(gleich > 500, "nur " + gleich + " gleiche Koepfe");
        assertTrue(verschieden > 500, "nur " + verschieden + " verschiedene Koepfe");
    }

    /** Ein Schluessel je Match: matches, todo und byKey halten dasselbe
     *  Objekt, ohne Kopie der Ref-Folge. Der Kandidat im Todo traegt die
     *  Nummer seines Matches, die angewandten sind Matches des Speichers. */
    @Test
    void einSchluesselJeMatch() {
        String json = """
            {"format":3,"name":"ein_schluessel","rules":[{
              "name":"A_2_B","rank":10,
              "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
              "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
              "corrs":[{"type":"AB","left":"a","right":"b","role":"establishes"}]
            }]}""";
        Graph g = new Graph();
        List<Rule> rules = Rules.load(json, g);
        g.addBaseline("a1", "A");
        g.addBaseline("a2", "A");
        Engine e = new Engine(rules);
        e.seed(g, Map.of());
        assertEquals(2, e.matches.size());
        Set<MatchKey> imTodo = Collections.newSetFromMap(new IdentityHashMap<>());
        for (Engine.TodoKey t : e.todo) {
            imTodo.add(t.key);
            assertSame(e.matches.get(t.mix).key, t.key);
        }
        Set<MatchKey> imIndex = Collections.newSetFromMap(new IdentityHashMap<>());
        imIndex.addAll(e.byKey.keySet());
        for (Engine.MatchRec m : e.matches) {
            assertTrue(imTodo.contains(m.key));
            assertTrue(imIndex.contains(m.key));
            assertSame(m.refs, m.key.refs);
            assertEquals(m.ruleIx, m.key.ruleIx);
        }
        while (e.step(g, Map.of()) != null) { /* bis zum Fixpunkt */ }
        assertFalse(e.applied.isEmpty());
        Set<MatchKey> imSpeicher = Collections.newSetFromMap(new IdentityHashMap<>());
        for (Engine.MatchRec m : e.matches) imSpeicher.add(m.key);
        for (MatchKey k : e.appliedKeys()) assertTrue(imSpeicher.contains(k));
    }
}
