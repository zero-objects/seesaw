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

  public   CreateNode(String typ, int parent, int leaf, Chain t) {
        this(typ, parent, leaf, t, null, -1, null, null, false);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst) {
        this(typ, parent, leaf, t, konst, -1, null, null, false);
    }

  public   CreateNode(String typ, int parent, int leaf, Chain t, String konst,
            int dynAnchor, String dynAttr, Chain dynTransform, boolean corrFullMatch) {
        this.typ = typ; this.parent = parent; derivedLeaf = leaf; derivedTransform = t;
        this.konst = konst; this.dynAnchor = dynAnchor; this.dynAttr = dynAttr;
        this.dynTransform = dynTransform; this.corrFullMatch = corrFullMatch;
    }
}
