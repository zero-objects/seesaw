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
    /**
     * Ist dieser Knoten eine Korrespondenz?
     *
     * <p>Eine Eigenschaft des Knotens, keine Engine-Buchhaltung: sie
     * ueberlebt {@code materialize} mit dem Knoten und ist das, woran
     * das Loeschen entlangläuft. Faellt eine Korrespondenz, fallen die
     * Elemente, die sie verbindet -- eine Korrespondenz, die eine
     * Uebersetzung bezeugt, deren Ergebnis fort ist, ist kein
     * zulaessiger Ruhezustand (Sandra 2026-08-11).
     */
    public boolean isCorr;

  public   Node(Id id, int typ, St status, Id src, Chain tr) {
        this(id, typ, status, src, tr, -1);
    }

  public   Node(Id id, int typ, St status, Id src, Chain tr, int konstIx) {
        this.id = id; this.typ = typ; this.status = status;
        this.derivedSource = src; this.derivedTransform = tr;
        this.konstIx = konstIx;
    }
}
