package net.sandrakessler.seesaw.ident;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotSame;

import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.Random;

import org.junit.jupiter.api.Test;

/** Id über vier Wörter: Ordnung, Gleichheit und Hash wie bei der Byte-Darstellung. */
class IdTest {

    /** Vorzeichenlos byteweise, die bisherige Ordnung. */
    private static int byteOrder(byte[] x, byte[] y) {
        for (int i = 0; i < 32; i++) {
            int a = x[i] & 0xFF, c = y[i] & 0xFF;
            if (a != c) return Integer.compare(a, c);
        }
        return 0;
    }

    private static byte[] random(Random r) {
        byte[] b = new byte[32];
        r.nextBytes(b);
        return b;
    }

    @Test
    void ordnungGleichByteOrdnungAufZufallspaaren() {
        Random r = new Random(4);
        for (int i = 0; i < 10_000; i++) {
            byte[] x = random(r), y = random(r);
            // Ein Teil der Paare teilt ein langes Präfix, damit spätere Wörter entscheiden.
            if (i % 2 == 0) {
                int k = r.nextInt(32);
                System.arraycopy(x, 0, y, 0, k);
            }
            assertEquals(Integer.signum(byteOrder(x, y)),
                    Integer.signum(new Id(x).compareTo(new Id(y))), "Paar " + i);
        }
    }

    @Test
    void ordnungAnDenVorzeichengrenzen() {
        // In jedem Wort und an jeder Byte-Stelle: 0x7f gegen 0x80 und 0x00 gegen 0xff.
        int[][] paare = {{0x7f, 0x80}, {0x00, 0xff}, {0x80, 0xff}, {0x00, 0x7f}};
        for (int pos = 0; pos < 32; pos++) {
            for (int[] p : paare) {
                byte[] x = new byte[32], y = new byte[32];
                x[pos] = (byte) p[0];
                y[pos] = (byte) p[1];
                Id ix = new Id(x), iy = new Id(y);
                assertEquals(-1, Integer.signum(ix.compareTo(iy)), "pos " + pos);
                assertEquals(1, Integer.signum(iy.compareTo(ix)), "pos " + pos);
                assertEquals(0, ix.compareTo(new Id(x.clone())));
            }
        }
        // Ganzes Wort: 0x7f… gegen 0x80… mit gleichem Rest.
        for (int w = 0; w < 4; w++) {
            byte[] x = new byte[32], y = new byte[32];
            x[8 * w] = 0x7f;
            y[8 * w] = (byte) 0x80;
            Arrays.fill(x, 8 * w + 1, 8 * w + 8, (byte) 0xff);
            assertEquals(-1, Integer.signum(new Id(x).compareTo(new Id(y))), "Wort " + w);
        }
    }

    @Test
    void gleichesW0VerschiedenerRestIstUngleichUndGetrennt() {
        Random r = new Random(5);
        Map<Id, Integer> map = new HashMap<>();
        byte[] kopf = new byte[8];
        r.nextBytes(kopf);
        int n = 0;
        for (int pos = 8; pos < 32; pos++) {
            byte[] b = new byte[32];
            System.arraycopy(kopf, 0, b, 0, 8);
            b[pos] = 1;
            Id id = new Id(b);
            assertNotEquals(new Id(new byte[32]) , id);
            map.put(id, pos);
            n++;
        }
        assertEquals(n, map.size());
        for (int pos = 8; pos < 32; pos++) {
            byte[] b = new byte[32];
            System.arraycopy(kopf, 0, b, 0, 8);
            b[pos] = 1;
            assertEquals(pos, map.get(new Id(b)));
        }
    }

    @Test
    void gleicheBytesSindGleichMitGleichemHash() {
        Random r = new Random(6);
        for (int i = 0; i < 1000; i++) {
            byte[] b = random(r);
            Id x = new Id(b), y = new Id(b.clone());
            assertEquals(x, y);
            assertEquals(x.hashCode(), y.hashCode());
        }
    }

    /** Die Iterationsfolge jeder HashMap mit Id-Schlüssel hängt am Hash: er bleibt Arrays.hashCode. */
    @Test
    void hashCodeGleichArraysHashCode() {
        Random r = new Random(7);
        for (int i = 0; i < 10_000; i++) {
            Id id = new Id(random(r));
            assertEquals(Arrays.hashCode(id.b), id.hashCode());
        }
        assertEquals(Arrays.hashCode(new byte[32]), new Id(new byte[32]).hashCode());
    }

    @Test
    void bytesLiefertKopie() {
        byte[] b = random(new Random(8));
        Id id = new Id(b);
        byte[] k = id.bytes();
        assertArrayEquals(b, k);
        assertNotSame(id.b, k);
        k[0] ^= 1;
        assertArrayEquals(b, id.bytes());
    }

    @Test
    void fromHexRundreise() {
        Id id = new Id(random(new Random(9)));
        StringBuilder sb = new StringBuilder();
        for (byte x : id.b) sb.append(String.format("%02x", x & 0xFF));
        assertEquals(id, Id.fromHex(sb.toString()));
    }
}
