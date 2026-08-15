package net.sandrakessler.seesaw.session;

import java.util.Arrays;
import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;


/**
 * Diagnose-Sweep (Stage 1b): uml_java-Forward-Cascade (Janus-Workload)
 * ueber wachsende UML-Modelle (N Klassen x M Attribute). Misst
 * Engine-Schritte (cascadeLen) UND In-Prozess-Kern-Zeit (seedRouted +
 * elementsAdded + step bis Saettigung), min-of-N. Modell-Aufbau via
 * DeltaCodec AUSSERHALB der Messung. Nur bei V2_UML=1;
 * Parameter via CLASSES (Liste), ATTRS, REPS.
 */
class UmlScaleTest {

    /** Delta-JSON: 1 Model, n Klassen, je m Attribute (name+type). */
    private static String buildDelta(int n, int m) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"origin\":\"User\",\"op_star\":[");
        sb.append("{\"type\":\"AddNode\",\"parent\":\"root\",\"childId\":\"mModel\","
                + "\"edgeType\":\"contains\",\"typeId\":\"Model\",\"attrs\":{}}");
        for (int i = 0; i < n; i++) {
            String c = "c" + i;
            sb.append(",{\"type\":\"AddNode\",\"parent\":\"mModel\",\"childId\":\"").append(c)
              .append("\",\"edgeType\":\"classes\",\"typeId\":\"Class\",\"attrs\":{\"name\":\"C")
              .append(i).append("\"}}");
            for (int j = 0; j < m; j++) {
                String a = c + "_a" + j;
                sb.append(",{\"type\":\"AddNode\",\"parent\":\"").append(c)
                  .append("\",\"childId\":\"").append(a)
                  .append("\",\"edgeType\":\"attributes\",\"typeId\":\"Attribute\","
                        + "\"attrs\":{\"name\":\"a").append(j).append("\",\"type\":\"String\"}}");
            }
        }
        sb.append("]}");
        return sb.toString();
    }

    @Test
    @EnabledIfEnvironmentVariable(named = "V2_UML", matches = "1")
    void sweep() throws Exception {
        int m = Integer.parseInt(System.getenv().getOrDefault("ATTRS", "3"));
        int reps = Integer.parseInt(System.getenv().getOrDefault("REPS", "5"));
        String classesEnv = System.getenv().getOrDefault("CLASSES", "50,100,200,400,800,1600");
        int[] classes = java.util.Arrays.stream(classesEnv.split(","))
                .mapToInt(s -> Integer.parseInt(s.trim())).toArray();

        System.out.printf("SWEEP-HEADER-UML attrs_per_class=%d reps=%d%n", m, reps);
        for (int n : classes) {
            String delta = buildDelta(n, m);
            long best = Long.MAX_VALUE;
            long steps = -1;
            int elements = -1;
            for (int rep = 0; rep < reps; rep++) {
                // Modell-Aufbau AUSSERHALB der Messung.
                Graph g = new Graph();
                SessionRules sr = SessionRules.load("uml_java", g);
                Map<Id, String> vals = new HashMap<>();
                DeltaCodec codec = new DeltaCodec(g, vals, sr);
                DeltaCodec.Result r = codec.apply(delta);
                if (!r.errors.isEmpty()) {
                    throw new AssertionError("delta errors: " + r.errors);
                }
                int el = g.allNodes().size();

                // Engine-Kern (Forward-Cascade bis Saettigung).
                long t0 = System.nanoTime();
                Engine e = new Engine(sr.rules);
                e.seedRouted(g, vals, List.copyOf(r.deltaTypes));
                e.elementsAdded(g, vals, r.newNodes);
                while (e.step(g, vals) != null) { /* Saettigung */ }
                long ns = System.nanoTime() - t0;

                if (ns < best) {
                    best = ns;
                    steps = e.cascadeLen;
                    elements = el;
                }
            }
            System.out.printf(
                    "SWEEP-UML classes=%-5d elements=%-8d steps=%-8d engine_ms=%.2f%n",
                    n, elements, steps, best / 1e6);
        }
    }
}
