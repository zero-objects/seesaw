package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Die Kennungen der Regelnamen je Graph ({@link Names}, Rust
 * {@code plan/names.rs}), Tests aus Rust {@code plan.rs}.
 */
final class NamesTest {

    /** Die Regel {@code Father_2_Male} (Rust {@code father_rule}). */
    private static final String FATHER = """
        {"format":3,"name":"plan_tests","rules":[{
          "name":"Father_2_Male","rank":850,
          "left":{"anchor":"fam","nodes":[
            {"name":"fam","type":"Family"},
            {"name":"father","type":"Father"},
            {"name":"member","type":"Member"},
            {"name":"first","type":"firstName"}
          ],"links":[["fam","father"],["father","member"],["member","first"]]},
          "right":{"anchor":"male","nodes":[
            {"name":"male","type":"Male"},
            {"name":"name","type":"name"}
          ],"links":[["male","name"]]},
          "corrs":[
            {"type":"PersonCorr","left":"member","right":"male","role":"establishes",
             "bindings":[{"left":"first","right":"name"}]}
          ]
        }]}""";

    /** {@code n} Familien mit Vater, Mitglied und Vornamen (Rust
     *  {@code seed}). */
    private static Graph seed(int n, Map<Id, String> vals) {
        Graph g = new Graph();
        for (int i = 0; i < n; i++) {
            Id f = g.addBaseline("f" + i, "Family");
            Id r = g.addBaseline("f" + i + "/father", "Father");
            Id m = g.addBaseline("f" + i + "/father/m", "Member");
            Id leaf = g.addBaseline("f" + i + "/father/m/fn", "firstName");
            g.connect(f, r, St.SOLID);
            g.connect(r, m, St.SOLID);
            g.connect(m, leaf, St.SOLID);
            vals.put(leaf, "John" + i);
        }
        return g;
    }

    /**
     * Eine Engine, die mit einem anderen Graphen weitermacht, benennt die
     * Typen ihrer Pläne in dessen Tabellen. Die Kopie entsteht vor dem
     * ersten Schritt, wenn der Korrespondenztyp noch nicht eingetragen
     * ist, und bekommt zuerst einen anderen Typ: die Nummer, die der
     * Korrespondenztyp im Original hat, gehört in der Kopie diesem anderen
     * Typ (Rust {@code a_plan_names_its_types_in_the_graph_at_hand}).
     */
    @Test
    void einPlanBenenntSeineTypenImGraphenInDerHand() {
        Map<Id, String> vals = new HashMap<>();
        Graph a = seed(2, vals);
        List<Rule> rules = Rules.load(FATHER, a);
        assertNull(a.lookup("PersonCorr"), "der Korrespondenztyp kommt mit dem ersten Schritt");
        Graph b = a.materialize();
        int zebra = b.intern("Zebra");

        Engine e = new Engine(rules);
        e.seed(a, vals);
        assertEquals(Boolean.TRUE, e.step(a, vals));
        assertEquals(Integer.valueOf(zebra), a.lookup("PersonCorr"), "dieselbe Nummer");

        // Der Rest der Kaskade in der Kopie.
        while (e.step(b, vals) != null) { }
        Integer corr = b.lookup("PersonCorr");
        assertNotNull(corr, "in der Kopie angelegt");
        assertNotEquals(zebra, corr.intValue());
        assertTrue(b.typeCount(corr) >= 1);
        assertEquals(0, b.typeCount(zebra), "kein Knoten ist ein Zebra");
    }

    private static Id id(int n) {
        byte[] b = new byte[32];
        Arrays.fill(b, (byte) n);
        return new Id(b);
    }

    /** Typ des Platzes 1 der Regel 1 (Planposition 1 des Rückwärtsplans). */
    private static int place(Names names) {
        return names.typ[names.createAt[1] + 1];
    }

    /**
     * Die Kennungen, die für die Namen einer Regel behalten werden,
     * gehören zu einem Graphen und einer Regelliste. Für andere entfallen
     * sie (Rust {@code kept_ids_are_dropped_for_other_tables_and_other_rules}).
     */
    @Test
    void behalteneKennungenEntfallenFuerAndereGraphenUndRegeln() {
        Map<Id, String> vals = new HashMap<>();
        Graph a = seed(1, vals);
        List<Rule> rules = Rules.load(FATHER, a);
        Names names = new Names();

        // Eine Kopie, bevor der Typ eingetragen ist, mit einem anderen Typ
        // unter der Nummer, die er im Original bekommt.
        Graph b = a.materialize();
        assertNotEquals(a.uid(), b.uid(), "eine Kopie ist ein anderer Graph");
        int other = b.intern("Other");

        // Regel 1 (der Rückwärtsplan), Planposition 1.
        names.forStep(a, rules, 1).addGhost(a, 1, id(1), "Fresh");
        int inA = a.lookup("Fresh");
        assertEquals(other, inA, "dieselbe Nummer");
        assertEquals(inA, a.node(id(1)).typ);
        // Behalten: der zweite Knoten derselben Planposition.
        names.forStep(a, rules, 1).addGhost(a, 1, id(2), "Fresh");
        assertEquals(inA, a.node(id(2)).typ);
        assertEquals(inA, place(names));
        // Die Plätze der anderen Regel sind ihre eigenen.
        assertEquals(Names.NONE, names.typ[names.createAt[0] + 1]);

        // Die Kopie: neu aufgelöst, in ihren Tabellen.
        names.forStep(b, rules, 1).addGhost(b, 1, id(1), "Fresh");
        int inB = b.lookup("Fresh");
        assertNotEquals(inA, inB);
        assertEquals(inB, b.node(id(1)).typ);
        assertEquals("Fresh", b.typeName(b.node(id(1)).typ));

        // Und wieder das Original.
        names.forStep(a, rules, 1).addGhost(a, 1, id(3), "Fresh");
        assertEquals(inA, a.node(id(3)).typ);

        // Eine andere Regelliste: nichts von der ersten bleibt.
        List<Rule> copy = new ArrayList<>(rules);
        names.forStep(a, copy, 1);
        assertEquals(Names.NONE, place(names));
        // Eine Regelliste, deren Länge sich geändert hat, ebenso.
        names.forStep(a, copy, 1).addGhost(a, 1, id(4), "Fresh");
        assertEquals(inA, place(names));
        copy.add(copy.get(0));
        names.forStep(a, copy, 1);
        assertEquals(Names.NONE, place(names));
    }
}
