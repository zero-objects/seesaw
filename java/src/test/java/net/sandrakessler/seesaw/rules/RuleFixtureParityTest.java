package net.sandrakessler.seesaw.rules;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.stream.Stream;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

/**
 * Beide Sprachen lesen dasselbe Regelformat — das ist Konzeption, nicht
 * Zufall: nur wenn Rust und Java dieselbe Regeldatei laden, misst der
 * Sprachvergleich die Sprache und nicht zwei verschiedene Regelsätze.
 *
 * <p>Die Regel-Fixtures liegen deshalb doppelt: einmal als Java-Test-
 * Ressource, einmal unter den Rust-Tests. Dieser Test haelt die beiden
 * Kopien Byte fuer Byte zusammen. Ohne ihn koennte eine Seite ihre
 * Kopie aendern, beide Testsuiten blieben gruen, und der Vergleich
 * liefe stillschweigend auf verschiedenen Eingaben.
 *
 * <p>Vorgaenger war {@code CrossModuleFixtureTest} in {@code core_java},
 * der dieselbe Frage fuer die erste Generation stellte. Er ist mit
 * seinem Gegenstand verschwunden: sein Exporter schreibt
 * {@code l_pattern}/{@code r_pattern}, der heutige Rust-Loader liest
 * {@code left}/{@code right}/{@code corrs}.
 */
class RuleFixtureParityTest {

    /** Java-Seite, relativ zur Modulwurzel. */
    private static final Path JAVA_FIXTURES =
            Path.of("src/test/resources/fixtures");

    /**
     * Rust-Seite. Zwei Kandidaten, weil das Modul in zwei Baeumen mit
     * verschiedenem Layout liegt: im Arbeitsbaum neben den Crates unter
     * {@code pilot/}, im veroeffentlichten Baum als {@code java/} neben
     * dem Crate-Wurzelverzeichnis. Faellt beides aus, schlaegt der Test
     * fehl — er ueberspringt nicht.
     */
    private static final List<Path> RUST_CANDIDATES = List.of(
            Path.of("../crates/seesaw-core/tests/fixtures/rules"),
            Path.of("../tests/fixtures/rules"));

    private static Path rustFixtures() {
        for (Path p : RUST_CANDIDATES) {
            if (Files.isDirectory(p)) {
                return p;
            }
        }
        return fail("Rust-Fixtures nicht gefunden, gesucht in: "
                + RUST_CANDIDATES + " (cwd " + Path.of("").toAbsolutePath() + ")");
    }

    @Test
    void geteilteRegeldateienSindByteGleich() throws IOException {
        Path rust = rustFixtures();
        List<String> verglichen = new ArrayList<>();

        try (Stream<Path> java = Files.list(JAVA_FIXTURES)) {
            for (Path j : java.sorted().toList()) {
                Path r = rust.resolve(j.getFileName());
                if (!Files.exists(r)) {
                    // Java-eigene Fixture (Golden-Werte, Seeds). Deren
                    // Gleichlauf pruefen die Golden-Tests ueber die Werte.
                    continue;
                }
                assertEquals(Files.readString(r), Files.readString(j),
                        j.getFileName() + " weicht zwischen Java und Rust ab");
                verglichen.add(j.getFileName().toString());
            }
        }

        // Ohne diese Schranke wuerde der Test auch dann gruen, wenn die
        // Namen auf einer Seite wandern und gar nichts mehr paart.
        assertTrue(verglichen.size() >= 2,
                "zu wenige geteilte Regeldateien gefunden: " + verglichen);
    }
}
