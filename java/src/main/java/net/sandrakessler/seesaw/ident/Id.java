package net.sandrakessler.seesaw.ident;

import java.util.Arrays;


// ── Id (32 Byte, lexikographisch vergleichbar wie Rust-GhostId) ──
public final class Id implements Comparable<Id> {
    public final byte[] b; // 32

    public Id(byte[] b) { this.b = b; }

    public static Id fromHex(String s) {
        byte[] out = new byte[32];
        for (int i = 0; i < 32; i++)
            out[i] = (byte) Integer.parseInt(s.substring(2 * i, 2 * i + 2), 16);
        return new Id(out);
    }

    @Override public int compareTo(Id o) {
        for (int i = 0; i < 32; i++) {
            int a = b[i] & 0xFF, c = o.b[i] & 0xFF;
            if (a != c) return Integer.compare(a, c);
        }
        return 0;
    }

    @Override public boolean equals(Object o) {
        return o instanceof Id && Arrays.equals(b, ((Id) o).b);
    }

    @Override public int hashCode() { return Arrays.hashCode(b); }
}
