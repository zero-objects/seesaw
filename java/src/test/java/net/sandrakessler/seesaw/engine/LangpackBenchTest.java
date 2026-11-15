package net.sandrakessler.seesaw.engine;

import java.util.Arrays;
import java.util.Map;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledIfEnvironmentVariable;

/**
 * Messung: der Sprachpack der Rust-Suite mit und ohne
 * {@link AnchorIndex}. Nur bei {@code SEESAW_LANGPACK=1}.
 *
 * <p>Parameter über die Umgebung: {@code KINDS} (700), {@code NODES}
 * (2000), {@code DELTA} (200), {@code CELLS} (Kommaliste aus add, seed,
 * backward, delta-add, delta-change, delta-delete), {@code INDEX} (both,
 * on, off), {@code REPS_ON} (7) und {@code REPS_OFF} (2). Gemessen wird
 * die Zeit der Messstrecke der Zelle im Prozess (Modellaufbau und Laden
 * der Regeln liegen außerhalb), je Stellung das Minimum und der Median
 * über die Wiederholungen. Die ersten Wiederholungen wärmen die JVM
 * auf.
 *
 * <p>Eine Zeile je Zelle und Stellung:
 * {@code LANGPACK-JAVA cell=… k=… n=… index=… reps=… min_ms=… median_ms=… first_ms=… steps=… applied=… nodes=… searches=… fp=…}
 * ({@code first_ms}: im schnellsten Lauf die Zeit des ersten Aufrufs,
 * also von {@code seed} oder {@code elementsAdded}, der Rest sind die
 * Schritte; {@code searches}: gestartete Suchen des ganzen Laufs,
 * unabhängig von der Last des Rechners).
 */
final class LangpackBenchTest {

    private static Langpack.Outcome run(String cell, Langpack.Spec spec, int delta, boolean index) {
        return switch (cell) {
            case "add" -> Langpack.add(spec, index);
            case "seed" -> Langpack.seed(spec, index);
            case "backward" -> Langpack.backward(spec, index);
            case "delta-add" -> Langpack.deltaAdd(spec, delta, index);
            case "delta-change" -> Langpack.deltaChange(spec, delta, index);
            case "delta-delete" -> Langpack.deltaDelete(spec, delta, index);
            default -> throw new IllegalArgumentException("unbekannte Zelle: " + cell);
        };
    }

    @Test
    @EnabledIfEnvironmentVariable(named = "SEESAW_LANGPACK", matches = "1")
    void messung() {
        Map<String, String> env = System.getenv();
        Langpack.Spec spec = new Langpack.Spec(
                Integer.parseInt(env.getOrDefault("KINDS", "700")),
                Integer.parseInt(env.getOrDefault("NODES", "2000")));
        int delta = Integer.parseInt(env.getOrDefault("DELTA", "200"));
        String which = env.getOrDefault("INDEX", "both");
        int repsOn = Integer.parseInt(env.getOrDefault("REPS_ON", "7"));
        int repsOff = Integer.parseInt(env.getOrDefault("REPS_OFF", "2"));
        for (String cell : env.getOrDefault("CELLS", "add,seed").split(",")) {
            for (boolean index : new boolean[] {true, false}) {
                if (index && which.equals("off") || !index && which.equals("on")) continue;
                int reps = index ? repsOn : repsOff;
                long[] nanos = new long[reps];
                Langpack.Outcome o = null;
                Langpack.Outcome best = null;
                for (int i = 0; i < reps; i++) {
                    o = run(cell.trim(), spec, delta, index);
                    nanos[i] = o.nanos();
                    if (best == null || o.nanos() < best.nanos()) best = o;
                }
                Arrays.sort(nanos);
                System.out.printf(
                        "LANGPACK-JAVA cell=%s k=%d n=%d index=%s reps=%d min_ms=%.1f median_ms=%.1f "
                                + "first_ms=%.1f steps=%d applied=%d nodes=%d searches=%d fp=%s%n",
                        cell.trim(), spec.kinds(), spec.nodes(), index ? "on" : "off", reps,
                        nanos[0] / 1e6, nanos[reps / 2] / 1e6, best.firstNanos() / 1e6, o.steps(),
                        o.applied(), o.nodes(), o.searches(), o.fingerprint());
            }
        }
    }
}
