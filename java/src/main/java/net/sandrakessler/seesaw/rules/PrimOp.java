package net.sandrakessler.seesaw.rules;

/** Primitiv-Art; {@code tag} = erstes Byte des Schritts. */
public enum PrimOp {
    IDENTITY(0),
    CAPITALIZE(1),
    DECAPITALIZE(2),
    PREFIX(3),
    SUFFIX(4),
    STRIP_PREFIX(5),
    STRIP_SUFFIX(6);

    public final int tag;

    PrimOp(int tag) {
        this.tag = tag;
    }
}
