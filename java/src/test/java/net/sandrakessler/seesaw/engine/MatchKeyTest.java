package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Random;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;

/**
 * Schluessel eines Matches: {@link Engine#key} bleibt fuer Hosts mit
 * derselben Ausgabe, die Engine selbst fuehrt {@link MatchKey} mit
 * derselben Gleichheit wie der alte Schluessel.
 */
final class MatchKeyTest {

    /** {@code Engine.key} von 2.0.2, unveraendert kopiert. */
    private static String alterKey(int ruleIx, Id[] refs) {
        StringBuilder sb = new StringBuilder().append(ruleIx);
        for (Id r : refs) {
            sb.append(':');
            for (byte b : r.b) {
                sb.append(HEX[(b >> 4) & 0xF]).append(HEX[b & 0xF]);
            }
        }
        return sb.toString();
    }

    private static final char[] HEX = "0123456789abcdef".toCharArray();

    private static Id id(int first) {
        byte[] b = new byte[32];
        for (int i = 0; i < 32; i++) b[i] = (byte) (first + i);
        return new Id(b);
    }

    @Test
    void keyGleichDerFassungVon202() {
        assertEquals("0", Engine.key(0, new Id[0]));
        assertEquals("12:000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
                + ":f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff000102030405060708090a0b0c0d0e0f",
                Engine.key(12, new Id[] {id(0), id(0xf0)}));
        Random rnd = new Random(7);
        Id[] pool = TodoKeyTest.pool(rnd, 40);
        for (int i = 0; i < 1000; i++) {
            int rule = rnd.nextInt(100_000);
            Id[] refs = TodoKeyTest.refs(rnd, pool, 4);
            assertEquals(alterKey(rule, refs), Engine.key(rule, refs));
        }
    }

    @Test
    void matchKeyGleichWieDerAlteSchluessel() {
        Random rnd = new Random(11);
        Id[] pool = TodoKeyTest.pool(rnd, 30);
        int gleich = 0;
        for (int i = 0; i < 2000; i++) {
            int ra = rnd.nextInt(4);
            Id[] a = TodoKeyTest.refs(rnd, pool, 3);
            int rb;
            Id[] b;
            switch (rnd.nextInt(4)) {
                case 0 -> { rb = ra; b = a.clone(); }
                case 1 -> { rb = rnd.nextInt(4); b = a.clone(); }
                case 2 -> {
                    rb = ra;
                    b = a.clone();
                    if (b.length > 0) b[rnd.nextInt(b.length)] = pool[rnd.nextInt(pool.length)];
                }
                default -> { rb = rnd.nextInt(4); b = TodoKeyTest.refs(rnd, pool, 3); }
            }
            boolean alt = alterKey(ra, a).equals(alterKey(rb, b));
            MatchKey x = new MatchKey(ra, a);
            MatchKey y = new MatchKey(rb, b);
            assertEquals(alt, x.equals(y), alterKey(ra, a) + " / " + alterKey(rb, b));
            assertEquals(alt, y.equals(x));
            if (alt) {
                gleich++;
                assertEquals(x.hashCode(), y.hashCode());
            }
        }
        assertTrue(gleich > 500, "nur " + gleich + " gleiche Paare");
        assertFalse(new MatchKey(0, new Id[0]).equals(null));
        assertFalse(new MatchKey(0, new Id[0]).equals("0"));
    }

    @Test
    void schluesselindexFuehrtMatchKey() {
        Id a = id(1);
        Id b = id(2);
        Engine e = new Engine(TodoKeyTest.rules(5, 5), false);
        e.record(0, new Id[] {a, b});
        e.record(0, new Id[] {a, b});
        e.record(1, new Id[] {a, b});
        e.record(0, new Id[] {b, a});
        assertEquals(3, e.matches.size());
        assertEquals(3, e.byKey.size());
        assertEquals(0, e.byKey.get(new MatchKey(0, new Id[] {a, b})));
        assertEquals(1, e.byKey.get(new MatchKey(1, new Id[] {a, b})));
        assertEquals(2, e.byKey.get(new MatchKey(0, new Id[] {b, a})));

        // Zurueckziehen vergisst den Schluessel, derselbe Match wird
        // danach neu eingetragen.
        e.retractMatch(new Graph(), 0, new ArrayList<>());
        assertNull(e.byKey.get(new MatchKey(0, new Id[] {a, b})));
        assertEquals(2, e.byKey.size());
        e.record(0, new Id[] {a, b});
        assertEquals(4, e.matches.size());
        assertEquals(3, e.byKey.get(new MatchKey(0, new Id[] {a, b})));
    }
}
