package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.session.Fixtures;

import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;

/**
 * Ein materialisiertes Erzeugnis wird zurückgezogen, ein
 * Baseline-Element nicht.
 *
 * <p>Spiegel von {@code materialisiertes_erzeugnis_wird_zurueckgezogen}
 * in {@code seesaw-core/tests/format.rs}. Beide Hälften zählen: die
 * erste ist der Fix vom 2026-08-10, bis dahin prüfte
 * {@code retractMatch} auf {@code GHOST} und ein gefaltetes Erzeugnis
 * blieb stehen. Die zweite hält fest, dass der Fix nicht zu viel
 * zurückzieht — was der Host selbst angelegt hat, steht in keiner
 * Produktmenge.
 *
 * <p>Knoten UND Kanten, weil der Fix beide betrifft.
 */
class RetractionAfterMaterializeTest {

    private static Id onlyOfType(Graph g, String typ) {
        Integer t = g.typesByName.get(typ);
        assertNotNull(t, "Typ " + typ + " muss existieren");
        for (Map.Entry<Id, Graph.Slot> e : g.map.entrySet()) {
            Graph.Slot s = e.getValue();
            if (s.node != null && s.node.typ == t && s.node.status.matchable()) {
                return e.getKey();
            }
        }
        throw new AssertionError("kein " + typ + "-Knoten");
    }

    @Test
    void erzeugnisFaelltAuchNachMaterialisierung() throws Exception {
        for (boolean materialisieren : new boolean[] {false, true}) {
            JsonNode rules = Fixtures.resource("/fixtures/uml_java_min.json");
            Graph g = new Graph();
            List<Rule> lowered = Rules.load(rules, g);

            Id model = g.addBaseline("m", "Model");
            Id cls = g.addBaseline("m/Person", "Class");
            Id cname = g.addBaseline("m/Person/name", "name");
            g.connect(model, cls, St.SOLID);
            g.connect(cls, cname, St.SOLID);
            Map<Id, String> vals = new HashMap<>();
            vals.put(cname, "Person");

            Engine e = new Engine(lowered);
            e.run(g, vals, 1000);

            Id jcls = onlyOfType(g, "JavaClass");
            Id corr = onlyOfType(g, "CorrClass");
            Id kante = null;
            for (Part p : g.parts(corr)) {
                if (p.outgoing && p.other.equals(jcls)) {
                    kante = p.connection;
                }
            }
            assertNotNull(kante, "Corr zeigt auf die JavaClass");

            String fall = "materialisiert=" + materialisieren;
            if (materialisieren) {
                g = g.materialize();
                assertEquals(St.SOLID, g.node(jcls).status,
                        "die Materialisierung macht das Erzeugnis solid");
                assertEquals(St.SOLID, g.conn(kante).status,
                        "die Materialisierung macht auch die Kante solid");
            }

            // Delta: die Quelle faellt weg.
            g.setNodeStatus(cls, St.TOMBSTONE);
            e.elementRemoved(cls);
            e.elementDeleted(g, cls);
            e.consolidate(g);

            assertEquals(St.TOMBSTONE, g.node(jcls).status,
                    fall + ": das Erzeugnis muss zurueckgezogen werden");
            assertEquals(St.TOMBSTONE, g.conn(kante).status,
                    fall + ": die erzeugte Kante muss mitfallen");

            // Die Gegenrichtung: Baseline-Elemente bleiben unberuehrt.
            assertEquals(St.SOLID, g.node(model).status,
                    fall + ": der Baseline-Anker bleibt unberuehrt");
            assertEquals(St.SOLID, g.node(cname).status,
                    fall + ": das Baseline-Blatt bleibt unberuehrt");
        }
    }

    /**
     * Löschen entlang der Korrespondenz: der Host löscht das Erzeugnis.
     *
     * <p>Spiegel von {@code geloeschtes_erzeugnis_reisst_die_quelle_mit}
     * in {@code seesaw-core/tests/format.rs}. Die Gegenrichtung zum Test
     * oben: dort fällt die Quelle und das Erzeugnis muss mitfallen, hier
     * fällt das Erzeugnis und die Quelle muss mitfallen.
     *
     * <p>Entscheidung Sandra 2026-08-11: wer die erzeugte Java-Klasse
     * löscht, löscht die UML-Klasse. Wer sie behalten will, nimmt die
     * Löschung zurück (Undo), statt dass der Sync einen Sonderfall kennt.
     */
    @Test
    void geloeschtesErzeugnisReisstDieQuelleMit() throws Exception {
        for (boolean materialisieren : new boolean[] {false, true}) {
            JsonNode rules = Fixtures.resource("/fixtures/uml_java_min.json");
            Graph g = new Graph();
            List<Rule> lowered = Rules.load(rules, g);

            Id model = g.addBaseline("m", "Model");
            Id cls = g.addBaseline("m/Person", "Class");
            Id cname = g.addBaseline("m/Person/name", "name");
            g.connect(model, cls, St.SOLID);
            g.connect(cls, cname, St.SOLID);
            Map<Id, String> vals = new HashMap<>();
            vals.put(cname, "Person");

            Engine e = new Engine(lowered);
            e.run(g, vals, 1000);

            Id jcls = onlyOfType(g, "JavaClass");
            Id corr = onlyOfType(g, "CorrClass");
            if (materialisieren) {
                g = g.materialize();
            }
            String fall = "materialisiert=" + materialisieren;

            // Delta: der Host löscht das ERZEUGNIS.
            g.setNodeStatus(jcls, St.TOMBSTONE);
            e.elementRemoved(jcls);
            e.elementDeleted(g, jcls);
            e.consolidate(g);

            assertEquals(St.TOMBSTONE, g.node(corr).status,
                    fall + ": die Korrespondenz darf keine Uebersetzung mehr bezeugen");
            assertEquals(St.TOMBSTONE, g.node(cls).status,
                    fall + ": die Quelle faellt entlang der Korrespondenz mit");
            assertEquals(St.SOLID, g.node(model).status,
                    fall + ": der Baseline-Anker bleibt unberuehrt");
        }
    }

    /**
     * Der Zweck der ganzen Reihe, als Test: der Host löscht ein
     * erzeugtes Blatt, und die Löschung trägt auf die Quellseite.
     *
     * <p>Spiegel von {@code geloeschtes_erzeugtes_blatt_traegt_zur_quelle}.
     * Das erzeugte Namensblatt ist kein Endpunkt der
     * Klassen-Korrespondenz. Es steht in einer EIGENEN Korrespondenz,
     * der Attribut-Korrespondenz zwischen den beiden Namensblättern
     * (Sandra 2026-08-12). Über die trägt die Löschung.
     */
    @Test
    void geloeschtesErzeugtesBlattTraegtZurQuelle() throws Exception {
        JsonNode rules = Fixtures.resource("/fixtures/uml_java_min.json");
        Graph g = new Graph();
        List<Rule> lowered = Rules.load(rules, g);

        Id model = g.addBaseline("m", "Model");
        Id cls = g.addBaseline("m/Person", "Class");
        Id cname = g.addBaseline("m/Person/name", "name");
        g.connect(model, cls, St.SOLID);
        g.connect(cls, cname, St.SOLID);
        Map<Id, String> vals = new HashMap<>();
        vals.put(cname, "Person");

        Engine e = new Engine(lowered);
        e.run(g, vals, 1000);

        Id jcls = onlyOfType(g, "JavaClass");
        Id jname = g.childLeafOfType(jcls, "name");
        assertNotNull(jname, "die JavaClass hat ein erzeugtes Namensblatt");
        Id acorr = onlyOfType(g, "CorrClass_name");

        g.setNodeStatus(jname, St.TOMBSTONE);
        e.elementRemoved(jname);
        e.elementDeleted(g, jname);
        e.consolidate(g);

        assertEquals(St.TOMBSTONE, g.node(acorr).status,
                "die Attribut-Korrespondenz darf kein fehlendes Blatt bezeugen");
        assertEquals(St.TOMBSTONE, g.node(cname).status,
                "das Quellblatt faellt entlang der Attribut-Korrespondenz mit");
        assertEquals(St.SOLID, g.node(model).status,
                "der Baseline-Anker bleibt unberuehrt");
    }

    /**
     * Der dynamische Fall: {@code left_type}/{@code right_type} statt
     * Musterposition. Spiegel von
     * {@code dynamische_bindung_erzeugt_blatt_korrespondenz}.
     *
     * <p>Die Quelle wird erst beim Anwenden über
     * {@code childLeafOfType} gefunden, die Attribut-Korrespondenz
     * entsteht deshalb dort statt im Lowering.
     */
    @Test
    void dynamischeBindungErzeugtBlattKorrespondenz() throws Exception {
        JsonNode rules = Fixtures.resource("/fixtures/uml_java_dyn.json");
        Graph g = new Graph();
        List<Rule> lowered = Rules.load(rules, g);

        Id model = g.addBaseline("m", "Model");
        Id cls = g.addBaseline("m/Person", "Class");
        Id cname = g.addBaseline("m/Person/name", "name");
        g.connect(model, cls, St.SOLID);
        g.connect(cls, cname, St.SOLID);
        Map<Id, String> vals = new HashMap<>();
        vals.put(cname, "Person");

        Engine e = new Engine(lowered);
        e.run(g, vals, 1000);

        Id jcls = onlyOfType(g, "JavaClass");
        Id jname = g.childLeafOfType(jcls, "name");
        assertNotNull(jname, "die JavaClass hat ein erzeugtes Namensblatt");
        assertEquals("Person", g.resolveValue(jname, vals),
                "das dynamisch gebundene Blatt traegt den Quellwert");

        Id acorr = onlyOfType(g, "CorrClass_name");
        boolean anQuelle = false, anZiel = false;
        for (Part p : g.parts(acorr)) {
            if (p.other.equals(cname)) anQuelle = true;
            if (p.other.equals(jname)) anZiel = true;
        }
        assertEquals(true, anQuelle && anZiel,
                "die Attribut-Korrespondenz haelt Quell- und Zielblatt");

        g.setNodeStatus(jname, St.TOMBSTONE);
        e.elementRemoved(jname);
        e.elementDeleted(g, jname);
        e.consolidate(g);
        assertEquals(St.TOMBSTONE, g.node(cname).status,
                "das Quellblatt faellt entlang der dynamischen Attribut-Korrespondenz");
        assertEquals(St.SOLID, g.node(model).status,
                "der Baseline-Anker bleibt unberuehrt");
    }
}
