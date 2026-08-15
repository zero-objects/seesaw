package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.session.Fixtures;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;


/**
 * E2 Schritt 2: das exportierte UML↔Java-Fixture
 * ({@code /fixtures/rules_uml_java.json}, main-Resources — die Session
 * braucht es produktiv) lädt über den Fixtures-Pfad, und ein
 * Mini-Forward (Class-Knoten → JavaClass + Corr) läuft auf der
 * Engine durch.
 */
class UmlJavaFixtureTest {

    @Test
    void fixtureLaedtAlleZwoelfGerichtetenRegeln() throws Exception {
        Graph g = new Graph();
        SessionRules sr = SessionRules.load("uml_java", g);
        assertEquals(12, sr.rules.size(), "6 Regeln × 2 Richtungen (+ R_Param)");
        assertTrue(sr.attrTypes.containsAll(
            List.of("name", "type", "returns", "param", "returnType",
                "isStatic", "index", "direction")));
        assertTrue(sr.corrTypes.contains("CorrClass"));
        assertTrue(sr.corrTypes.contains("CorrParam"),
            "CorrParam-Korrespondenztyp (R_Param) geladen");
        assertEquals("classes", sr.edgeKindByCombo.get(
            SessionRules.comboKey("Model", "Class")));
        assertEquals("ownedParameters", sr.edgeKindByCombo.get(
            SessionRules.comboKey("Operation", "Parameter")));
        assertEquals("hasParameter", sr.edgeKindByCombo.get(
            SessionRules.comboKey("JavaMethod", "JavaParam")));
        assertTrue(sr.reifiedKinds.isEmpty(),
            "UML↔Java-Satz hat nur Direct-Kanten");
    }

    @Test
    void produktJarTraegtAlleDreiRessourcenpfade() throws Exception {
        assertNotNull(UmlJavaFixtureTest.class.getResource(
            "/fixtures/rules_uml_java.json"));
        assertNotNull(UmlJavaFixtureTest.class.getResource(
            "/rules/rules_uml_java.json"));
        assertNotNull(UmlJavaFixtureTest.class.getResource(
            "/v2/rules_uml_java.json"));
    }

    @Test
    void miniForwardClassZuJavaClassMitCorr() throws Exception {
        Graph g = new Graph();
        SessionRules sr = SessionRules.load("uml_java", g);
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, sr);

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"mModel",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"mModel","childId":"cFoo",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}}
            ]}""");
        assertTrue(r.errors.isEmpty(), "" + r.errors);

        Engine e = new Engine(sr.rules);
        e.seedRouted(g, vals, List.copyOf(r.deltaTypes));
        e.elementsAdded(g, vals, r.newNodes);
        while (e.step(g, vals) != null) { /* Sättigung */ }

        // JavaClass materialisiert, Name über das abgeleitete Blatt.
        Integer jcT = g.lookup("JavaClass");
        assertNotNull(jcT, "JavaClass-Typ interniert");
        List<Id> jcs = g.nodesOfType(jcT);
        assertEquals(1, jcs.size(), "genau eine JavaClass");
        assertEquals("Foo", g.resolveValue(
            g.childLeafOfType(jcs.get(0), "name"), vals));

        // Corr-Kette: Class → CorrClass → JavaClass (Spec §1.3).
        Id cFoo = Ident.identBaseline("cFoo");
        Id corr = null;
        Integer corrT = g.lookup("CorrClass");
        assertNotNull(corrT);
        for (Part p : g.partsByOtherType(cFoo, corrT)) {
            if (p.outgoing) corr = p.other;
        }
        assertNotNull(corr, "Class trägt den CorrClass");
        assertTrue(g.connected(corr, jcs.get(0)), "Corr → JavaClass");

        // Kein Ping-Pong: R_Class← erkennt die Übersetzung (corr_recognition)
        // und erzeugt KEINE zweite UML-Class.
        Integer clsT = g.lookup("Class");
        assertEquals(1, g.nodesOfType(clsT).size(), "genau eine Class");
    }
}
