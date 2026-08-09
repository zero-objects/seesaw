package net.sandrakessler.seesaw.graph;

import net.sandrakessler.seesaw.rules.Chain;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;

// ── Graph ──
public final class Node {
    public final Id id;
    public final int typ;
    public St status;
    public final Id derivedSource; // null wenn kein DerivedLeaf
    public final Chain derivedTransform;
    /** Regel-Konstanten-Referenz (KonstTable), -1 = keine. */
    public final int konstIx;

  public   Node(Id id, int typ, St status, Id src, Chain tr) {
        this(id, typ, status, src, tr, -1);
    }

  public   Node(Id id, int typ, St status, Id src, Chain tr, int konstIx) {
        this.id = id; this.typ = typ; this.status = status;
        this.derivedSource = src; this.derivedTransform = tr;
        this.konstIx = konstIx;
    }
}
