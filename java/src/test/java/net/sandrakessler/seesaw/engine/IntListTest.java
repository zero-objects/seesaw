package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Random;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.ident.Id;

/** {@link IntList} gegen {@code ArrayList<Integer>}, die Liste, die
 *  {@code byElement} bisher je Element hielt. */
final class IntListTest {

    @Test
    void wieArrayListUeberDieGrenzeVonVierHinaus() {
        Random rnd = new Random(3);
        for (int n = 0; n <= 40; n++) {
            IntList l = new IntList();
            List<Integer> alt = new ArrayList<>();
            for (int i = 0; i < n; i++) {
                int v = i % 7 == 3 ? alt.isEmpty() ? 0 : alt.get(0) : rnd.nextInt(Integer.MAX_VALUE);
                l.add(v);
                alt.add(v);
                assertEquals(alt.size(), l.size());
                for (int j = 0; j < alt.size(); j++) assertEquals(alt.get(j), l.get(j));
            }
            assertEquals(alt.toString(), l.toString());
            int[] a = new int[alt.size()];
            for (int j = 0; j < a.length; j++) a[j] = alt.get(j);
            assertArrayEquals(a, l.toArray());
            int size = n;
            assertThrows(IndexOutOfBoundsException.class, () -> l.get(size));
            assertThrows(IndexOutOfBoundsException.class, () -> l.get(-1));
        }
    }

    @Test
    void zustandDerEngineGleichDerListeDerMatches() {
        // Jede Id steht in byElement mit den Indizes der Matches, die sie
        // enthalten, in der Reihenfolge des Eintragens, auch doppelt.
        Random rnd = new Random(5);
        Id[] pool = TodoKeyTest.pool(rnd, 12);
        Engine e = new Engine(TodoKeyTest.rules(5, 1), false);
        Map<Id, List<Integer>> alt = new HashMap<>();
        for (int i = 0; i < 400; i++) {
            int ruleIx = rnd.nextInt(2);
            Id[] refs = TodoKeyTest.refs(rnd, pool, 3);
            int vorher = e.matches.size();
            e.record(ruleIx, refs);
            if (e.matches.size() == vorher) continue;
            for (Id id : refs) {
                alt.computeIfAbsent(id, x -> new ArrayList<>()).add(vorher);
            }
        }
        // Dieselben Schluessel in derselben Folge des Eintragens: auch die
        // Iterationsfolge der HashMap ist gleich.
        assertEquals(new ArrayList<>(alt.keySet()), new ArrayList<>(e.byElement.keySet()));
        for (Map.Entry<Id, List<Integer>> x : alt.entrySet()) {
            assertEquals(x.getValue().toString(), e.byElement.get(x.getKey()).toString());
        }
    }
}
