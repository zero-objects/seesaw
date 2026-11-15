package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

/**
 * Der Sprachpack der Rust-Suite in der Größe {@code k50/n2000}: Java
 * muss je Zelle dieselben Schritte, dieselbe Knotenzahl und denselben
 * Fingerprint liefern wie Rust. Die erwarteten Werte stammen aus
 * {@code benches/suite/results/2026-11-15-codepit-final/fingerprints-2.0.2-5dce323.txt}
 * (dort gleich für Rust 2.0.2 und den Stand mit Index).
 */
final class LangpackTest {
    private static final Langpack.Spec SPEC = new Langpack.Spec(50, 2000);

    private static void same(String cell, Langpack.Outcome o, long steps, long applied, long nodes,
            String fingerprint) {
        assertEquals(steps + " " + applied + " " + nodes + " " + fingerprint,
                o.steps() + " " + o.applied() + " " + o.nodes() + " " + o.fingerprint(),
                cell + ": Schritte, angewandte Schritte, Knoten, Fingerprint");
    }

    /** Mit Index und auf dem Weg von 2.0.2. Der Index spart Suchen,
     *  sonst wäre er nicht im Spiel. */
    @Test
    void addWieRust() {
        Langpack.Outcome indexed = Langpack.add(SPEC, true);
        Langpack.Outcome plain = Langpack.add(SPEC, false);
        same("add mit Index", indexed, 5246, 2623, 12554, "e636a88ace0d48eb");
        same("add ohne Index", plain, 5246, 2623, 12554, "e636a88ace0d48eb");
        // Im Prüfmodus fährt die Engine mit Index beide Wege, die Zahl
        // sagt dann nichts.
        if (!Engine.CHECKS) {
            assertTrue(indexed.searches() * 10 < plain.searches(),
                    "Suchen mit Index " + indexed.searches() + ", ohne " + plain.searches());
        }
    }

    /** Bei dieser Größe wählt die Schwelle den Seed über den Index. */
    @Test
    void seedWieRust() {
        Langpack.Outcome indexed = Langpack.seed(SPEC, true);
        Langpack.Outcome plain = Langpack.seed(SPEC, false);
        same("seed mit Index", indexed, 5246, 2623, 12554, "e636a88ace0d48eb");
        same("seed ohne Index", plain, 5246, 2623, 12554, "e636a88ace0d48eb");
        assertEquals(1, indexed.seedsByIndex(), "der Seed ging nicht über den Index");
        assertEquals(0, plain.seedsByIndex());
    }

    @Test
    void backwardWieRust() {
        same("backward mit Index", Langpack.backward(SPEC, true), 2623, 2623, 11874,
                "4ce9f6fa8155c574");
    }

    @Test
    void deltaAddWieRust() {
        same("delta-add mit Index", Langpack.deltaAdd(SPEC, 200, true), 278, 2901, 13854,
                "6274c10049129e52");
    }

    @Test
    void deltaChangeWieRust() {
        same("delta-change mit Index", Langpack.deltaChange(SPEC, 200, true), 2182, 4805, 12770,
                "d81f82cba13a48a4");
    }

    @Test
    void deltaDeleteWieRust() {
        same("delta-delete mit Index", Langpack.deltaDelete(SPEC, 200, true), 0, 2623, 12554,
                "acc610a627b68a27");
    }
}
