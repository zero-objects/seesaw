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
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

/**
 * Messkampagnen-Harness: Paper-Skala (654k Seed-Knoten) auf dem
 * Java-Port — Äquivalenz (11 512 / 492 377) + In-Prozess-Zeiten je
 * Phase. Nur bei V2_PAPER=1 (Fixture ist 258 MB, gitignored;
 * Export: hermann_export_java_fixtures in Rust).
 *
 * <p>Seit 2026-08-08 zusätzlich {@code alive_nodes} und der
 * Endzustands-Fingerprint, wie in {@link EquivalenceTest} für mini und
 * astra. Vorher las dieser Test die paper-Einträge aus
 * {@code expected.json}, prüfte aber nur die Schrittzahl — der
 * Fingerprint stand in der Datei, ohne dass ihn je etwas verglich. Eine
 * abweichende Identitäts-Ableitung auf Paper-Skala wäre unsichtbar
 * geblieben.
 */
class PaperScaleTest {

    @Test
    @EnabledIfEnvironmentVariable(named = "V2_PAPER", matches = "1")
    void paperSkala() throws Exception {
        JsonNode expected = Fixtures.resource("/fixtures/expected.json").get("paper");
        Graph g = new Graph();
        long t0 = System.nanoTime();
        Map<Id, String> values = Fixtures.seed("paper", g);
        System.err.printf("JAVA-PAPER seed_s=%.1f%n", (System.nanoTime() - t0) / 1e9);
        for (JsonNode phase : expected) {
            String phaseName = phase.get("phase").asText();
            long want = phase.get("steps").asLong();
            List<Rule> rules = Fixtures.rules(phaseName, g);
            Engine e = new Engine(rules);
            long p0 = System.nanoTime();
            e.runToSaturation(g, values);
            Fingerprint.Result fp = Fingerprint.of(g);
            System.err.printf("JAVA-PAPER %-12s steps=%d alive=%d fp=%s s=%.1f%n",
                    phaseName, e.cascadeLen, fp.aliveNodes, fp.hex,
                    (System.nanoTime() - p0) / 1e9);
            assertEquals(want, e.cascadeLen, phaseName + " steps");
            assertEquals(phase.get("alive_nodes").asInt(), fp.aliveNodes,
                    phaseName + " alive_nodes");
            assertEquals(phase.get("fingerprint").asText(), fp.hex,
                    phaseName + " fingerprint");
        }
    }
}
