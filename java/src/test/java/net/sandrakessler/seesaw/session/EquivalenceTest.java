package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

/**
 * Etappe 6: Java-Port des Datenmodells muss die Anwendungs-Zahlen
 * der Rust-Engine EXAKT reproduzieren (hermann-Satz, beide Phasen,
 * bidirektional). Erwartungswerte aus dem Rust-Fixture-Export.
 *
 * <p>Seit Task 5c zusätzlich der Endzustands-Fingerprint je Phase: die
 * Anwendungs-Zahl allein haengt nicht an den Ids, eine abweichende
 * Identitaets-Ableitung bliebe unsichtbar. Der Vergleichswert kommt aus
 * dem Rust-Lauf ({@code expected.json}), nicht aus einem Java-Golden.
 */
class EquivalenceTest {

    long runPhase(Graph g, Map<Id, String> values, String phase) throws Exception {
        List<Rule> rules = Fixtures.rules(phase, g);
        Engine e = new Engine(rules);
        e.runToSaturation(g, values);
        return e.cascadeLen;
    }

    void profil(String name) throws Exception {
        JsonNode expected = Fixtures.resource("/fixtures/expected.json").get(name);
        Graph g = new Graph();
        Map<Id, String> values = Fixtures.seed(name, g);
        for (JsonNode phase : expected) {
            String phaseName = phase.get("phase").asText();
            long got = runPhase(g, values, phaseName);
            assertEquals(phase.get("steps").asLong(), got, name + "/" + phaseName + " steps");
            Fingerprint.Result fp = Fingerprint.of(g);
            assertEquals(phase.get("alive_nodes").asInt(), fp.aliveNodes,
                    name + "/" + phaseName + " alive_nodes");
            assertEquals(phase.get("fingerprint").asText(), fp.hex,
                    name + "/" + phaseName + " fingerprint");
        }
    }

    @Test
    void miniAequivalenz() throws Exception {
        profil("mini");
    }

    @Test
    void astraAequivalenz() throws Exception {
        profil("astra");
    }
}
