package net.sandrakessler.seesaw.ident;

import java.util.Arrays;


// ── Id (32 Byte, lexikographisch vergleichbar wie Rust-GhostId) ──
//
// Neben den 32 Byte hält die Id die gleichen Bytes als vier long (w0 bis w3,
// Big-Endian) und den Hash, beides einmal im Konstruktor berechnet. Vergleich
// und Gleichheit laufen über die vier Wörter. Der Hash ist unverändert
// Arrays.hashCode(b): die Iterationsfolge jeder HashMap/HashSet mit Id als
// Schlüssel hängt an diesem Wert und ist im Ergebnis sichtbar (Graph,
// SnapshotCodec, Engine).
//
// b bleibt öffentlich und wird ohne Kopie gehalten. Wer b liest, darf es nicht
// verändern: Wörter und Hash sind daraus abgeleitet und würden nicht folgen.
public final class Id implements Comparable<Id> {
    public final byte[] b; // 32
    private final long w0, w1, w2, w3;
    private final int hash;

    public Id(byte[] b) {
        if (b.length != 32) {
            throw new IllegalArgumentException("Id braucht 32 Byte, bekam " + b.length);
        }
        this.b = b;
        this.w0 = wort(b, 0);
        this.w1 = wort(b, 8);
        this.w2 = wort(b, 16);
        this.w3 = wort(b, 24);
        this.hash = Arrays.hashCode(b);
    }

    private static long wort(byte[] b, int o) {
        return ((long) (b[o] & 0xFF) << 56)
                | ((long) (b[o + 1] & 0xFF) << 48)
                | ((long) (b[o + 2] & 0xFF) << 40)
                | ((long) (b[o + 3] & 0xFF) << 32)
                | ((long) (b[o + 4] & 0xFF) << 24)
                | ((long) (b[o + 5] & 0xFF) << 16)
                | ((long) (b[o + 6] & 0xFF) << 8)
                | (long) (b[o + 7] & 0xFF);
    }

    public static Id fromHex(String s) {
        byte[] out = new byte[32];
        for (int i = 0; i < 32; i++)
            out[i] = (byte) Integer.parseInt(s.substring(2 * i, 2 * i + 2), 16);
        return new Id(out);
    }

    /** Die 32 Byte als Kopie. */
    public byte[] bytes() {
        return b.clone();
    }

    @Override public int compareTo(Id o) {
        // Big-Endian-Wörter vorzeichenlos vergleichen entspricht der Byte-Ordnung.
        int c = Long.compareUnsigned(w0, o.w0);
        if (c != 0) return c;
        c = Long.compareUnsigned(w1, o.w1);
        if (c != 0) return c;
        c = Long.compareUnsigned(w2, o.w2);
        if (c != 0) return c;
        return Long.compareUnsigned(w3, o.w3);
    }

    @Override public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof Id)) return false;
        Id x = (Id) o;
        return w0 == x.w0 && w1 == x.w1 && w2 == x.w2 && w3 == x.w3;
    }

    @Override public int hashCode() { return hash; }
}
