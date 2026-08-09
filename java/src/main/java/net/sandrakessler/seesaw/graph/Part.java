package net.sandrakessler.seesaw.graph;

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
}
