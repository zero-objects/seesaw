package net.sandrakessler.seesaw.ident;

import net.sandrakessler.seesaw.rules.Chain;

import java.nio.charset.StandardCharsets;

import net.sandrakessler.seesaw.hash.Blake3;

/**
 * Identitaets-Ableitung: blake3 ueber Struktur und Herkunft.
 *
 * <p><b>Eindeutige Kodierung.</b> Jedem Feld VARIABLER Laenge geht
 * seine Laenge als little-endian {@code u32} voraus. Felder fester
 * Breite (32-Byte-Ids, der Planindex) werden roh gehasht, weil das
 * Domaenen-Tag am Anfang die Struktur bereits festlegt.
 *
 * <p>Ohne die Laengenpraefixe ist die Kodierung nicht injektiv, und
 * zwar nicht als Hash-Kollision, sondern schon vor dem Hashen: bei
 * zwei benachbarten variablen Feldern ergeben {@code ("ab","c")} und
 * {@code ("a","bc")} dieselben Eingabebytes. Zwei verschiedene Knoten
 * teilten dann eine Identitaet und fielen still zusammen. Im Review
 * vom 2026-08-10 gefunden, fuer jede Ableitung auf einmal behoben,
 * damit die Regel ein Satz bleibt statt einer Fallunterscheidung.
 *
 * <p>Muss Byte fuer Byte mit {@code seesaw-core/src/graph.rs::ident}
 * uebereinstimmen.
 */
public final class Ident {
    private Ident() {}

    /** Ein Feld variabler Laenge: erst die Laenge, dann der Inhalt. */
    private static byte[] var(byte[] bytes) {
        byte[] out = new byte[4 + bytes.length];
        int n = bytes.length;
        out[0] = (byte) n;
        out[1] = (byte) (n >> 8);
        out[2] = (byte) (n >> 16);
        out[3] = (byte) (n >> 24);
        System.arraycopy(bytes, 0, out, 4, bytes.length);
        return out;
    }

    public static Id hashParts(byte[]... parts) {
        Blake3 h = new Blake3();
        for (byte[] p : parts) h.update(p);
        byte[] out = new byte[32];
        h.finalize32(out);
        return new Id(out);
    }

    public static byte[] utf8(String s) { return s.getBytes(StandardCharsets.UTF_8); }

    public static Id identBaseline(String external) {
        return hashParts(utf8("V2B\0"), var(utf8(external)));
    }

    public static Id identGhost(Id parent, String typName) {
        return hashParts(utf8("V2G\0"), parent.b, var(utf8(typName)));
    }

    /** Spiegel von {@code ident::derived}: die Transformation geht als
     *  kanonische Byte-Folge ihrer KETTE ein, nicht als Tag-Byte. */
    public static Id identDerived(Id parent, String typName, Id source, Chain t) {
        return hashParts(
                utf8("V2D\0"), parent.b, var(utf8(typName)), source.b, var(t.identBytes()));
    }

    public static Id identConnection(Id source, Id target) {
        return hashParts(utf8("V2C\0"), source.b, target.b);
    }

    /** Corr-Knoten: Anker + Typ + Match-Digest (Spec §1.4). */
    public static Id identCorr(Id anchor, String typName, Id[] refs) {
        Blake3 h = new Blake3();
        h.update(utf8("V2R\0"));
        h.update(anchor.b);
        h.update(var(utf8(typName)));
        // Die Ref-Liste traegt ihre ANZAHL, keine Bytelaenge: ohne sie
        // wuerden ein laengerer Typname und ein Ref weniger gleich
        // hashen.
        int n = refs.length;
        h.update(new byte[] {(byte) n, (byte) (n >> 8), (byte) (n >> 16), (byte) (n >> 24)});
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
        return hashParts(
                utf8("V2K\0"), parent.b, var(utf8(typName)), var(utf8(ruleName)), le);
    }
}
