package net.sandrakessler.seesaw.bench;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.engine.Langpack;

/**
 * Workload „Families to Persons“ der Rust-Suite in der Größe
 * {@code f2000}: Java muss je Zelle dieselben Schritte, angewandten
 * Schritte, Knoten und denselben Fingerprint liefern wie Rust. Die
 * erwarteten Werte stammen aus
 * {@code benches/suite/results/2026-11-15-auswahl/gate.txt} (Teil
 * {@code b3}, gleich auf x86_64 und s390x).
 */
final class F2pTest {

    private static void same(String cell, Langpack.Outcome o, long steps, long applied, long nodes,
            String fingerprint) {
        assertEquals(steps + " " + applied + " " + nodes + " " + fingerprint,
                o.steps() + " " + o.applied() + " " + o.nodes() + " " + o.fingerprint(),
                cell + ": Schritte, angewandte Schritte, Knoten, Fingerprint");
    }

    @Test
    void fingerprintsMatchRust() {
        same("f2p/add/f2000", F2p.add(2000, new Section()), 24002, 8001, 58006,
                "c3b4f34453abdb46");
        same("f2p/delta-delete/f2000/d1000", F2p.deltaDelete(2000, 1000, new Section()), 0, 8001,
                58006, "cc1b2254c3a11d0d");
    }
}
