package net.sandrakessler.seesaw.session;

import java.util.Arrays;
import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.File;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

/**
 * Diagnose-Sweep (Stage 1a, Java-Seite): hermann init+translation auf
 * der Engine über die vom Rust-Export gelieferten Seeds
 * (seed_f&lt;N&gt;.json im SWEEP_DIR). In-Prozess-Kern-Zeit (seed+step),
 * min-of-N, Regel-Lowering + Seed-Laden AUSSERHALB der Messung.
 * Nur bei V2_SWEEP=1; Parameter via SWEEP_DIR/REPS/FILES.
 */
class ScaleSweepTest {

    private static long runPhase(Graph g, Map<Id, String> vals, List<Rule> rules) {
        Engine e = new Engine(rules);
        e.runToSaturation(g, vals);
        return e.cascadeLen;
    }

    @Test
    @EnabledIfEnvironmentVariable(named = "V2_SWEEP", matches = "1")
    void sweep() throws Exception {
        String sweepDir = System.getenv().getOrDefault("SWEEP_DIR", "/tmp/hermann_sweep");
        int reps = Integer.parseInt(System.getenv().getOrDefault("REPS", "5"));
        String filesEnv = System.getenv().getOrDefault("FILES", "8,16,33,66,132,264");
        int[] files = java.util.Arrays.stream(filesEnv.split(","))
                .mapToInt(s -> Integer.parseInt(s.trim())).toArray();
        ObjectMapper mapper = new ObjectMapper();

        System.out.printf("SWEEP-HEADER-JAVA reps=%d dir=%s%n", reps, sweepDir);
        for (int f : files) {
            Path seedPath = Path.of(sweepDir, "seed_f" + f + ".json");
            JsonNode seedRoot = mapper.readTree(new File(seedPath.toString()));

            long best = Long.MAX_VALUE;
            long initApps = -1;
            long trApps = -1;
            int nodeCount = -1;
            for (int rep = 0; rep < reps; rep++) {
                Graph g = new Graph();
                Map<Id, String> vals = Fixtures.seedFromNode(seedRoot, g);
                List<Rule> initRules = Fixtures.rules("init", g);
                List<Rule> trRules = Fixtures.rules("translation", g);
                int nc = g.allNodes().size();

                long t0 = System.nanoTime();
                long ia = runPhase(g, vals, initRules);
                long ta = runPhase(g, vals, trRules);
                long ns = System.nanoTime() - t0;

                if (ns < best) {
                    best = ns;
                    initApps = ia;
                    trApps = ta;
                    nodeCount = nc;
                }
            }
            System.out.printf(
                    "SWEEP-JAVA files=%-4d nodes=%-8d init_apps=%-6d tr_apps=%-8d engine_ms=%.2f%n",
                    f, nodeCount, initApps, trApps, best / 1e6);
        }
    }
}
