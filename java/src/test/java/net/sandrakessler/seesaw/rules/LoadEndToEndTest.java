package net.sandrakessler.seesaw.rules;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;

import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

/**
 * Ende zu Ende: Regeldatei laden, senken, Kaskade fahren.
 *
 * <p>Dieselbe Fixture und dieselben Erwartungen wie
 * {@code seesaw-core/tests/format.rs} auf der Rust-Seite. Wenn dieser
 * Test grün ist, geht die volle Kette Datei → Validierung →
 * Erzeugungspläne → Engine in beiden Sprachen gleich aus.
 */
class LoadEndToEndTest {

    /** Lebende Knoten eines Typs. */
    private static int countOfType(Graph g, String typ) {
        Integer t = g.typesByName.get(typ);
        if (t == null) {
            return 0;
        }
        int n = 0;
        for (Graph.Slot s : g.map.values()) {
            if (s.node != null && s.node.typ == t && s.node.status.matchable()) {
                n++;
            }
        }
        return n;
    }

    /** Der einzige lebende Knoten eines Typs. */
    private static Id onlyOfType(Graph g, String typ) {
        Integer t = g.typesByName.get(typ);
        assertNotNull(t, "Typ " + typ + " muss existieren");
        Id found = null;
        for (Map.Entry<Id, Graph.Slot> e : g.map.entrySet()) {
            Graph.Slot s = e.getValue();
            if (s.node != null && s.node.typ == t && s.node.status.matchable()) {
                assertEquals(null, found, "mehr als ein " + typ + "-Knoten");
                found = e.getKey();
            }
        }
        assertNotNull(found, "kein " + typ + "-Knoten");
        return found;
    }

    @Test
    void regeldateiTreibtEineKaskade() throws Exception {
        String json = Resources.read("/fixtures/uml_java_min.json").toString();

        Graph g = new Graph();
        List<Rule> lowered = Rules.load(json, g);
        assertEquals(2, lowered.size(), "eine Regel, zwei Richtungen");
        assertEquals("R_Class→", lowered.get(0).name);
        assertEquals("R_Class←", lowered.get(1).name);

        // Seed: Model -> Class -> name-Blatt.
        Id model = g.addBaseline("m", "Model");
        Id cls = g.addBaseline("m/Person", "Class");
        Id cname = g.addBaseline("m/Person/name", "name");
        g.connect(model, cls, St.SOLID);
        g.connect(cls, cname, St.SOLID);

        Map<Id, String> vals = new HashMap<>();
        vals.put(cname, "Person");

        Engine e = new Engine(lowered);
        e.run(g, vals, 1000);

        assertEquals(1, countOfType(g, "JavaClass"), "genau eine Java-Klasse");
        assertEquals(1, countOfType(g, "CorrClass"), "genau eine Korrespondenz");

        // Das abgeleitete Namensblatt loest auf den Quellwert auf
        // (Bindung cname -> jname, transform: [] = Identitaet).
        Id jcls = onlyOfType(g, "JavaClass");
        Id jname = g.childLeafOfType(jcls, "name");
        assertNotNull(jname, "JavaClass hat ein name-Blatt");
        assertEquals("Person", g.resolveValue(jname, vals),
                "Namensblatt muss auf den Quellwert aufloesen");

        // Idempotenz: ein zweiter Lauf mit FRISCHEM Engine-Zustand darf
        // nichts Neues erzeugen. Die Konvergenz muss aus der
        // Graph-Identitaet kommen, nicht aus dem Gedaechtnis der ersten
        // Instanz.
        Engine e2 = new Engine(lowered);
        e2.run(g, vals, 1000);
        assertEquals(1, countOfType(g, "JavaClass"), "zweiter Lauf erzeugt keine weitere Klasse");
        assertEquals(1, countOfType(g, "CorrClass"), "zweiter Lauf erzeugt keine weitere Corr");
    }
}
