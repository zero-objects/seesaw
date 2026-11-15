package net.sandrakessler.seesaw.bench;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.nio.file.Files;
import java.nio.file.Path;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.engine.Langpack;

/**
 * Nachfahrt des Protokolls {@code open2} (zerodds-pde öffnet zwei
 * Dateien) in Java: jeder Aufruf muss das Ergebnis des Protokolls
 * liefern, das Ergebnis des Laufs muss gleich der Rust-Zelle
 * {@code pde/open/open2} sein (Label {@code sel} in
 * {@code benches/suite/results/2026-11-15-auswahl/uid-cgu1.tsv}).
 *
 * <p>Die Protokolle liegen in keinem Repository. Ohne
 * {@code SEESAW_PDE_DUMP} wird der Test übersprungen.
 */
final class PdeReplayTest {

    @Test
    void open2MatchesRecord() {
        String dir = System.getenv(PdeReplay.DUMP);
        assumeTrue(dir != null, PdeReplay.DUMP + " nicht gesetzt");
        Path trace = Path.of(dir, "open2.trace");
        assumeTrue(Files.isRegularFile(trace), trace + " fehlt");
        Langpack.Outcome o = PdeReplay.run(trace, new Section());
        assertEquals("5266 1923 16016 410a5645c91f535a",
                o.steps() + " " + o.applied() + " " + o.nodes() + " " + o.fingerprint(),
                "pde/open/open2: Schritte, angewandte Schritte, Knoten, Fingerprint");
    }
}
