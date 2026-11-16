package net.sandrakessler.seesaw.bench;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.nio.file.Files;
import java.nio.file.Path;

import org.junit.jupiter.api.Test;

/**
 * Nachfahrt des Protokolls {@code open2} (zerodds-pde öffnet zwei
 * Dateien) in Java. Rust liefert dafür die Zelle {@code pde/open/open2}
 * mit {@code 5266 1923 16016 410a5645c91f535a} (Label {@code sel} in
 * {@code benches/suite/results/2026-11-15-auswahl/uid-cgu1.tsv}).
 *
 * <p>Java 2.0.2 weicht an Zeile 27636 ab, und dieser Test hält genau
 * diese erste Abweichung fest: jeder Aufruf davor stimmt mit dem
 * Protokoll überein. Grund, offen zur Entscheidung: Java 2.0.2 nimmt
 * {@code SAME_VALUE} in den Suchplan ({@code Matcher.buildPlan}), Rust
 * nicht ({@code src/engine/matcher.rs} v2.0.2 Zeilen 128 und 144). An der
 * Engine {@code test-unit} findet Java dadurch den Match von
 * {@code TestUnit_JUnit5_Plan} (vorwärts) nicht, den Rust als Duplikat
 * erkennt. Dieselbe Erwartung führt {@code java/bench/pde-abweichungen.txt}
 * für {@code jgate.sh pde}.
 *
 * <p>Die Protokolle liegen in keinem Repository. Ohne
 * {@code SEESAW_PDE_DUMP} wird der Test übersprungen.
 */
final class PdeReplayTest {

    @Test
    void open2DeviatesAtKnownLine() {
        String dir = System.getenv(PdeReplay.DUMP);
        assumeTrue(dir != null, PdeReplay.DUMP + " nicht gesetzt");
        Path trace = Path.of(dir, "open2.trace");
        assumeTrue(Files.isRegularFile(trace), trace + " fehlt");
        PdeReplay.Mismatch m = assertThrows(PdeReplay.Mismatch.class,
                () -> PdeReplay.run(trace, new Section()));
        assertEquals(27636, m.line, "Zeile der ersten Abweichung");
        assertEquals(trace + ":27636: step: erwartet false, bekommen true; Aufruf: step 163 85 169"
                + " F 2 15954", m.getMessage(), "Meldung der ersten Abweichung");
    }
}
