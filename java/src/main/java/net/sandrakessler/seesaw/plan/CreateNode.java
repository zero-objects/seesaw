package net.sandrakessler.seesaw.plan;

import net.sandrakessler.seesaw.rules.Chain;

import java.util.regex.Pattern;


/** Ref: matched (>=0 Pattern-Position) oder new (~ix kodiert als -ix-1). */
public final class CreateNode {
    public final String typ;
    public final int parent;
    public final int derivedLeaf; // -1 wenn kein DerivedLeaf
    public final Chain derivedTransform;
    /** Regel-Konstante (§3b.4), null = keine. */
    public final String konst;
    /** Dynamisches Binding: {ankerPos, attrTyp, transform}, null = keins. */
    public final int dynAnchor;
    public final String dynAttr;
    public final Chain dynTransform;
    /** Corr-Knoten (Match-Digest-Identität). */
    public final boolean corrFullMatch;
    /**
     * Ist dieser erzeugte Knoten die Korrespondenz seiner Regel?
     *
     * <p>Vom Lowering aus {@code corrs} gesetzt. Die Engine folgt ihr
     * beim Loeschen: eine gefallene Korrespondenz nimmt die Elemente
     * mit, die sie verbindet.
     */
    public final boolean isCorr;
    /**
     * Typ der Attribut-Korrespondenz dieses Blattes, wenn es aus einem
     * DYNAMISCHEN Constraint stammt ({@code left_type}/{@code
     * right_type}), sonst null.
     *
     * <p>Im statischen Fall legt das Lowering die Blatt-Korrespondenz
     * selbst an, weil beide Blaetter als Musterposition feststehen. Im
     * dynamischen Fall wird die Quelle erst beim Anwenden ueber
     * {@code childLeafOfType} gefunden, also entsteht die
     * Korrespondenz dort -- anderer Ort, gleiche Sache.
     */
    public final String attrCorr;

  public   CreateNode(String typ, int parent, int leaf, Chain t) {
        this(typ, parent, leaf, t, null, -1, null, null, false, false);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, boolean isCorr) {
        this(typ, parent, leaf, t, null, -1, null, null, false, isCorr);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst) {
        this(typ, parent, leaf, t, konst, -1, null, null, false, false);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst,
            int dynAnchor, String dynAttr, Chain dynTransform, boolean corrFullMatch) {
        this(typ, parent, leaf, t, konst, dynAnchor, dynAttr, dynTransform, corrFullMatch, false);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst,
            int dynAnchor, String dynAttr, Chain dynTransform, boolean corrFullMatch,
            boolean isCorr) {
        this(typ, parent, leaf, t, konst, dynAnchor, dynAttr, dynTransform, corrFullMatch,
                isCorr, null);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst,
            int dynAnchor, String dynAttr, Chain dynTransform, boolean corrFullMatch,
            boolean isCorr, String attrCorr) {
        this.typ = typ; this.parent = parent; derivedLeaf = leaf; derivedTransform = t;
        this.konst = konst; this.dynAnchor = dynAnchor; this.dynAttr = dynAttr;
        this.dynTransform = dynTransform; this.corrFullMatch = corrFullMatch;
        this.isCorr = isCorr; this.attrCorr = attrCorr;
    }
}
