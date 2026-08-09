package net.sandrakessler.seesaw.rules;

import net.sandrakessler.seesaw.ident.Ident;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.Collections;
import java.util.Comparator;
import java.util.List;


/**
 * Spiegel von {@code PlanTransform}: entweder eine Kette oder die
 * PIL2SPELL-Comparator-Tabelle (die einzige Vokabel ohne Kettenform).
 * Die Kette wird beim Bauen NORMALISIERT — sonst hashen {@code []}
 * und {@code [Identity]} verschieden.
 */
public final class Chain {

    /** Normalform der Kette. */
    public final List<Prim> prims;

    private Chain(List<Prim> prims) {
        this.prims = prims;
    }

    public static final Chain IDENTITY = chain(Collections.emptyList());

    /** Kette aus rohen Schritten; normalisiert. */
    public static Chain chain(List<Prim> raw) {
        return new Chain(Collections.unmodifiableList(normalize(raw)));
    }


    /**
     * Spiegel von {@code Chain::normalized}: (1) Identity streichen,
     * (2) Affix-Schritte mit leerem Argument streichen,
     * (3) benachbarte gleichartige Affix-Schritte zusammenziehen —
     * die Argument-Reihenfolge je Art wie in Rust.
     */
    private static List<Prim> normalize(List<Prim> raw) {
        ArrayList<Prim> out = new ArrayList<>(raw.size());
        for (Prim p : raw) {
            if (p.op == PrimOp.IDENTITY) continue;
            if (p.arg.isEmpty() && p.op != PrimOp.CAPITALIZE && p.op != PrimOp.DECAPITALIZE) continue;
            Prim last = out.isEmpty() ? null : out.get(out.size() - 1);
            Prim merged = null;
            if (last != null && last.op == p.op) {
                switch (p.op) {
                    case PREFIX: merged = new Prim(PrimOp.PREFIX, p.arg + last.arg); break;
                    case SUFFIX: merged = new Prim(PrimOp.SUFFIX, last.arg + p.arg); break;
                    case STRIP_PREFIX:
                        merged = new Prim(PrimOp.STRIP_PREFIX, last.arg + p.arg); break;
                    case STRIP_SUFFIX:
                        merged = new Prim(PrimOp.STRIP_SUFFIX, p.arg + last.arg); break;
                    default: break; // Case-Operationen bleiben stehen
                }
            }
            if (merged != null) out.set(out.size() - 1, merged);
            else out.add(p);
        }
        return out;
    }

    /** Spiegel von {@code PlanTransform::ident_bytes}. */
    public byte[] identBytes() {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        out.write(0);
        for (Prim p : prims) {
            out.write(p.op.tag);
            byte[] a = Ident.utf8(p.arg);
            out.write(a.length);
            out.write(a.length >> 8);
            out.write(a.length >> 16);
            out.write(a.length >> 24);
            out.write(a, 0, a.length);
        }
        return out.toByteArray();
    }

    /** Wendet die Kette in Listenreihenfolge an; null = nicht anwendbar. */
    public String apply(String v) {
        String cur = v;
        for (Prim p : prims) {
            cur = p.apply(cur);
            if (cur == null) return null;
        }
        return cur;
    }

    /**
     * Spiegel von {@code Chain::invert_checked}: Quelle zu einem
     * Zielwert, aber nur wenn die VORWÄRTS-Kette diese Quelle
     * wieder auf genau diesen Zielwert abbildet. Kriterium ist
     * Konsistenz mit dem Zielwert, nicht Gleichheit mit einem
     * Original, das die Rückrichtung nicht kennt.
     *
     * <p>null = kein konsistenter Quellwert. Zwei Gründe, beide
     * gleich zu behandeln: die Rückwärts-Kette ist nicht anwendbar,
     * oder der Zielwert ist vorwärts gar nicht erreichbar
     * (Beispiel: Kette {@code [Capitalize, Prefix("get")]},
     * Zielwert {@code "getname"} — invers ergibt {@code "name"},
     * vorwärts daraus aber {@code "getName"}).
     */
    public String invertChecked(String target) {
        String source = inverse().apply(target);
        if (source == null) return null;
        return target.equals(apply(source)) ? source : null;
    }

    /** Spiegel von {@code Chain::inverse}: elementweise rueckwaerts. */
    public Chain inverse() {
        ArrayList<Prim> inv = new ArrayList<>(prims.size());
        for (int i = prims.size() - 1; i >= 0; i--) {
            inv.add(prims.get(i).inverse());
        }
        return chain(inv);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof Chain && ((Chain) o).prims.equals(prims);
    }

    @Override
    public int hashCode() {
        return prims.hashCode();
    }

    @Override
    public String toString() {
        return prims.toString();
    }
}
