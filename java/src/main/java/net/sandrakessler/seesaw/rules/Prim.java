package net.sandrakessler.seesaw.rules;


/** Ein Schritt der Kette; {@code arg} nur bei den Affix-Arten belegt. */
public final class Prim {
    public final PrimOp op;
    public final String arg;

    public Prim(PrimOp op, String arg) {
        this.op = op;
        this.arg = arg == null ? "" : arg;
        // Nur die Affix-Arten tragen ein Argument. Rust kann
        // `Capitalize("x")` gar nicht schreiben (das Argument hängt
        // dort an der Variante), Java könnte es — und würde es in
        // die Identität hashen, ohne dass es je eine Entsprechung
        // gäbe.
        if (!this.arg.isEmpty() && op != PrimOp.PREFIX && op != PrimOp.SUFFIX
                && op != PrimOp.STRIP_PREFIX && op != PrimOp.STRIP_SUFFIX) {
            throw new IllegalArgumentException(op + " trägt kein Argument: " + this.arg);
        }
    }

    public Prim(PrimOp op) { this(op, ""); }

    /**
     * Spiegel von {@code Prim::apply}; null = nicht anwendbar (strikt).
     *
     * <p>Öffentlich, damit ein Test die ROHE Kette Schritt für
     * Schritt anwenden kann. Ohne das ließe sich nicht prüfen, ob
     * die Normalisierung wirkungserhaltend ist, weil jede Kette
     * über {@link Chain#chain} bereits normalisiert entsteht.
     */
    public String apply(String v) {
        switch (op) {
            case IDENTITY: return v;
            case CAPITALIZE: return changeFirst(v, true);
            case DECAPITALIZE: return changeFirst(v, false);
            case PREFIX: return arg + v;
            case SUFFIX: return v + arg;
            case STRIP_PREFIX:
                return v.startsWith(arg) ? v.substring(arg.length()) : null;
            case STRIP_SUFFIX:
                return v.endsWith(arg) ? v.substring(0, v.length() - arg.length()) : null;
            default: throw new IllegalStateException();
        }
    }

    /** Spiegel von {@code Prim::inverse}. */
    public Prim inverse() {
        switch (op) {
            case IDENTITY: return this;
            case CAPITALIZE: return new Prim(PrimOp.DECAPITALIZE);
            case DECAPITALIZE: return new Prim(PrimOp.CAPITALIZE);
            case PREFIX: return new Prim(PrimOp.STRIP_PREFIX, arg);
            case SUFFIX: return new Prim(PrimOp.STRIP_SUFFIX, arg);
            case STRIP_PREFIX: return new Prim(PrimOp.PREFIX, arg);
            case STRIP_SUFFIX: return new Prim(PrimOp.SUFFIX, arg);
            default: throw new IllegalStateException();
        }
    }

    @Override public boolean equals(Object o) {
        return o instanceof Prim && ((Prim) o).op == op && ((Prim) o).arg.equals(arg);
    }

    @Override public int hashCode() { return op.hashCode() * 31 + arg.hashCode(); }

    @Override public String toString() {
        return arg.isEmpty() ? op.toString() : op + "(" + arg + ")";
    }

    /** Spiegel von {@code change_first}: erstes ZEICHEN (Code-Point,
     *  nicht char) umschreiben, Rest unverändert. Locale.ROOT, weil
     *  Rust {@code char::to_uppercase} sprachunabhängig ist. */
    public static String changeFirst(String s, boolean upper) {
        if (s.isEmpty()) return "";
        int cp = s.codePointAt(0);
        String head = new String(Character.toChars(cp));
        head = upper ? head.toUpperCase(java.util.Locale.ROOT)
                     : head.toLowerCase(java.util.Locale.ROOT);
        return head + s.substring(Character.charCount(cp));
    }
}
