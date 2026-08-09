package net.sandrakessler.seesaw.plan;

/**
 * Eine Bedingung zwischen zwei Pattern-Positionen, Spiegel von
 * {@code engine::matcher::Link}.
 */
public final class PatLink {
    /** Art der Bedingung. */
    public enum Kind {
        /** Gerichtete Verbindung von {@code from} nach {@code to}. */
        DIRECTED,
        /** Verbindung in irgendeiner Richtung. */
        CONTEXT,
        /**
         * Wert-Gleichheit zweier Blaetter. Die Werte bleiben im
         * Original, verglichen wird nur beim Matchen.
         */
        SAME_VALUE
    }

    public final int from;
    public final int to;
    public final Kind kind;

    public PatLink(int from, int to, Kind kind) {
        this.from = from;
        this.to = to;
        this.kind = kind;
    }

    /** Alt-Konstruktor: {@code true} = Kontext, {@code false} = gerichtet. */
    public PatLink(int from, int to, boolean context) {
        this(from, to, context ? Kind.CONTEXT : Kind.DIRECTED);
    }

    public static PatLink directed(int from, int to) {
        return new PatLink(from, to, Kind.DIRECTED);
    }

    public static PatLink context(int from, int to) {
        return new PatLink(from, to, Kind.CONTEXT);
    }

    public static PatLink sameValue(int a, int b) {
        return new PatLink(a, b, Kind.SAME_VALUE);
    }

    /** Nur fuer die Alt-Aufrufer, die {@code context} als Feld lasen. */
    public boolean isContext() {
        return kind == Kind.CONTEXT;
    }
}
