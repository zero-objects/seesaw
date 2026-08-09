package net.sandrakessler.seesaw.ident;

import net.sandrakessler.seesaw.rules.Chain;

import java.nio.charset.StandardCharsets;

import net.sandrakessler.seesaw.hash.Blake3;

/** Identitaets-Ableitung: blake3 ueber Struktur und Herkunft. */
public final class Ident {
    private Ident() {}

    public static Id hashParts(byte[]... parts) {
        Blake3 h = new Blake3();
        for (byte[] p : parts) h.update(p);
        byte[] out = new byte[32];
        h.finalize32(out);
        return new Id(out);
    }

    public static byte[] utf8(String s) { return s.getBytes(StandardCharsets.UTF_8); }

    public static Id identBaseline(String external) {
        return hashParts(utf8("V2B\0"), utf8(external));
    }

    public static Id identGhost(Id parent, String typName) {
        return hashParts(utf8("V2G\0"), parent.b, utf8(typName));
    }

    /** Spiegel von {@code ident::derived}: die Transformation geht als
     *  kanonische Byte-Folge ihrer KETTE ein, nicht als Tag-Byte. */
    public static Id identDerived(Id parent, String typName, Id source, Chain t) {
        return hashParts(utf8("V2D\0"), parent.b, utf8(typName), source.b, t.identBytes());
    }

    public static Id identConnection(Id source, Id target) {
        return hashParts(utf8("V2C\0"), source.b, target.b);
    }

    /** Corr-Knoten: Anker + Typ + Match-Digest (Spec §1.4). */
    public static Id identCorr(Id anchor, String typName, Id[] refs) {
        Blake3 h = new Blake3();
        h.update(utf8("V2R\0"));
        h.update(anchor.b);
        h.update(utf8(typName));
        for (Id r : refs) h.update(r.b);
        byte[] out = new byte[32];
        h.finalize32(out);
        return new Id(out);
    }

    /** Regel-Konstante: Identität über die ERZEUGENDE Regel, nie über den Wert. */
    public static Id identKonst(Id parent, String typName, String ruleName, int planIx) {
        byte[] le = new byte[] {
            (byte) planIx, (byte) (planIx >> 8), (byte) (planIx >> 16), (byte) (planIx >> 24),
        };
        return hashParts(utf8("V2K\0"), parent.b, utf8(typName), utf8(ruleName), le);
    }
}
