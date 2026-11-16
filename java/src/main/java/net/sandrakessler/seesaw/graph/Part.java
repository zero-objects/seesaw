package net.sandrakessler.seesaw.graph;

import java.util.Arrays;

import net.sandrakessler.seesaw.ident.Id;

/** Beteiligung; Ordnung = (otherTyp, other, outgoing, connection). */
public final class Part implements Comparable<Part> {
    public final int otherTyp;
    public final Id other;
    public final boolean outgoing;
    public final Id connection;

  public   Part(int ot, Id o, boolean out, Id c) {
        otherTyp = ot; other = o; outgoing = out; connection = c;
    }

    @Override public int compareTo(Part p) {
        int c = Integer.compare(otherTyp, p.otherTyp);
        if (c != 0) return c;
        c = other.compareTo(p.other);
        if (c != 0) return c;
        c = Boolean.compare(outgoing, p.outgoing);
        if (c != 0) return c;
        return connection.compareTo(p.connection);
    }

    // ── Feste Grenzen der Typbereiche ──

    private static final Id NULL_ID = new Id(new byte[32]);
    /** Bis zu diesem Typ (ausschließlich) liegen die Grenzen fest vor. */
    private static final int BOUNDS_MAX = 1 << 16;
    /** Grenze je Typ ab 0, ohne Lücken. Wächst unter der Klassensperre
     *  durch Kopie, ein Eintrag ändert sich nie. */
    private static volatile Part[] bounds = new Part[0];

    /**
     * Untere Grenze des Bereichs {@code otherTyp == typ} in einer nach
     * Part geordneten Menge: kleiner oder gleich jeder Participation mit
     * diesem Gegentyp, größer als jede mit kleinerem Gegentyp. Der
     * Bereich eines Typs liegt damit von {@code lowerBound(typ)}
     * einschließlich bis {@code lowerBound(typ + 1)} ausschließlich.
     *
     * <p>Für Typen von 0 bis {@link #BOUNDS_MAX} ein festes Objekt, ein
     * Aufruf legt nichts an. Andere Typen bekommen ein neues.
     */
    static Part lowerBound(int typ) {
        Part[] b = bounds;
        if (typ >= 0 && typ < b.length) return b[typ];
        return lowerBoundSlow(typ);
    }

    private static synchronized Part lowerBoundSlow(int typ) {
        if (typ < 0 || typ >= BOUNDS_MAX) return new Part(typ, NULL_ID, false, NULL_ID);
        Part[] b = bounds;
        if (typ < b.length) return b[typ];
        int len = Math.min(BOUNDS_MAX, Math.max(typ + 1, Math.max(64, 2 * b.length)));
        Part[] nb = Arrays.copyOf(b, len);
        for (int i = b.length; i < len; i++) nb[i] = new Part(i, NULL_ID, false, NULL_ID);
        bounds = nb;
        return nb[typ];
    }
}
