package net.sandrakessler.seesaw.graph;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.Prim;
import net.sandrakessler.seesaw.rules.PrimOp;

/**
 * {@link Graph#resolveValue} gegen die Fassung von Java 2.0.2 (hier
 * kopiert): Baseline-Blatt mit und ohne Wert, Regelkonstante,
 * abgeleitete Blätter über eine bis drei Stufen, eine Stufe, die nicht
 * anwendbar ist, eine fehlende Quelle und eine unbekannte Id.
 */
final class ResolveValueTest {

    /** resolveValue von Java 2.0.2, unverändert. */
    private static String alt(Graph g, Id id, Map<Id, String> values) {
        ArrayList<Chain> chain = new ArrayList<>();
        Id cur = id;
        while (true) {
            Node n = g.node(cur);
            if (n == null) return null;
            if (n.konstIx >= 0) {
                return altApply(chain, g.konstValues.get(n.konstIx));
            }
            if (n.derivedSource == null) {
                String v = values.get(cur);
                if (v == null) return null;
                return altApply(chain, v);
            }
            chain.add(n.derivedTransform);
            cur = n.derivedSource;
        }
    }

    private static String altApply(List<Chain> chain, String value) {
        String v = value;
        for (int i = chain.size() - 1; i >= 0; i--) {
            v = chain.get(i).apply(v);
            if (v == null) return null;
        }
        return v;
    }

    private static Chain chain(Prim... prims) {
        return Chain.chain(List.of(prims));
    }

    @Test
    void wieInJava202() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id owner = g.addBaseline("owner", "C");
        Id name = g.addBaseline("owner/name", "name");
        vals.put(name, "foo");
        Id ohneWert = g.addBaseline("owner/leer", "name");
        Id konst = g.addKonstLeaf(owner, "kind", "R", 0, "klasse");
        Id d1 = g.addDerivedLeaf(owner, "getter", name, chain(new Prim(PrimOp.CAPITALIZE),
                new Prim(PrimOp.PREFIX, "get")));
        Id d2 = g.addDerivedLeaf(owner, "feld", d1, chain(new Prim(PrimOp.SUFFIX, "Feld")));
        Id d3 = g.addDerivedLeaf(owner, "konst", d2, chain(new Prim(PrimOp.STRIP_PREFIX, "get")));
        // Auf einer Regelkonstante, und eine Stufe, die nicht anwendbar ist.
        Id dk = g.addDerivedLeaf(owner, "aufKonst", konst, chain(new Prim(PrimOp.SUFFIX, "_x")));
        Id nein = g.addDerivedLeaf(owner, "nein", d2, chain(new Prim(PrimOp.STRIP_PREFIX, "set")));
        Id ueberNein = g.addDerivedLeaf(owner, "ueberNein", nein, chain(new Prim(PrimOp.SUFFIX, "y")));
        Id aufLeer = g.addDerivedLeaf(owner, "aufLeer", ohneWert, chain(new Prim(PrimOp.SUFFIX, "y")));
        Id fehlt = g.addDerivedLeaf(owner, "fehlt", Ident.identBaseline("nirgends"),
                chain(new Prim(PrimOp.SUFFIX, "y")));
        Id unbekannt = Ident.identBaseline("unbekannt");

        assertEquals("foo", g.resolveValue(name, vals));
        assertNull(g.resolveValue(ohneWert, vals));
        assertEquals("klasse", g.resolveValue(konst, vals));
        assertEquals("getFoo", g.resolveValue(d1, vals));
        assertEquals("getFooFeld", g.resolveValue(d2, vals));
        assertEquals("FooFeld", g.resolveValue(d3, vals));
        assertEquals("klasse_x", g.resolveValue(dk, vals));
        assertNull(g.resolveValue(nein, vals));
        assertNull(g.resolveValue(ueberNein, vals));
        assertNull(g.resolveValue(aufLeer, vals));
        assertNull(g.resolveValue(fehlt, vals));
        assertNull(g.resolveValue(unbekannt, vals));
        assertNull(g.resolveValue(owner, vals), "Knoten ohne Wert");
        for (Id id : List.of(name, ohneWert, konst, d1, d2, d3, dk, nein, ueberNein, aufLeer, fehlt,
                unbekannt, owner)) {
            assertEquals(alt(g, id, vals), g.resolveValue(id, vals));
        }
    }
}
