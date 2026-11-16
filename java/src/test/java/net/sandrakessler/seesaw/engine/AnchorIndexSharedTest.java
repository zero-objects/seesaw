package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Der Vorrat der Indizes im Prozess (Rust {@code engine/anchor/own.rs}):
 * eine Engine ohne Index des Hosts nimmt den Index eines Regelsatzes
 * gleichen Inhalts, der im Prozess schon gebaut ist.
 *
 * <p>Die Tests fragen, was der Vorrat des Prozesses hält, einer leert
 * ihn. Sie laufen nacheinander (JUnit ohne parallele Ausführung). Andere
 * Tests nutzen den Vorrat nur, ein Index, der unter ihnen verworfen
 * wird, wird neu gebaut.
 */
final class AnchorIndexSharedTest {

    /** Eine Regel, {@code value} ist die Konstante ihrer Blätter (Rust
     *  {@code rules_json}). */
    private static String rulesJson(String value) {
        String leaf = "{\"name\":\"%s\",\"type\":\"%s\",\"predicate\":{\"kind\":\"equals\","
                + "\"value\":\"" + value + "\"},\"constant\":\"" + value + "\"}";
        return """
            {"format":3,"name":"own_tests","rules":[{
              "name":"Root","rank":900,
              "left":{"anchor":"n","nodes":[
                {"name":"f","type":"F"},
                {"name":"n","type":"J"},
                %s
              ],"links":[["f","n"],["n","n_rule"]]},
              "right":{"anchor":"m","nodes":[
                {"name":"p","type":"P","context":true},
                {"name":"m","type":"S"},
                %s
              ],"links":[["p","m"],["m","m_feature"]]},
              "corrs":[
                {"type":"CN","left":"n","right":"m","role":"establishes"},
                {"type":"CF","left":"f","right":"m","role":"establishes"}
              ]
            }]}""".formatted(leaf.formatted("n_rule", "rule"), leaf.formatted("m_feature", "feature"));
    }

    /** Regeln und der Index, den eine Engine über sie bei ihrer ersten
     *  Expansion nimmt. */
    private record Taken(AnchorIndex index, List<Rule> rules) {}

    /** Ein Graph mit einem Knoten, die Regeln in ihn geladen, und der
     *  Index, den eine Engine über sie bei ihrer ersten Expansion nimmt
     *  (Rust {@code index_of}). */
    private static Taken indexOf(String value) {
        Graph g = new Graph();
        Id f = g.addBaseline("file", "F");
        List<Rule> rules = Rules.load(rulesJson(value), g);
        Engine e = new Engine(rules, true);
        e.elementsAdded(g, Map.of(), List.of(f));
        assertNotNull(e.anchorIndex(), "die Engine hat keinen Index genommen");
        return new Taken(e.anchorIndex(), rules);
    }

    /** Zweimal geladene Regeln sind zwei Listen mit demselben Inhalt:
     *  beide Engines lesen einen Index. Andere Regeln bekommen einen
     *  anderen (Rust {@code an_equal_rule_set_takes_the_index_that_is_there}). */
    @Test
    void gleicherRegelsatzNimmtDenVorhandenenIndex() {
        Taken first = indexOf("shared-a");
        Taken second = indexOf("shared-a");
        assertNotSame(first.rules(), second.rules());
        assertSame(first.index(), second.index(), "ein Index für den gleichen Regelsatz");
        Taken other = indexOf("shared-b");
        assertNotSame(first.index(), other.index(), "ein anderer Index für andere Regeln");
    }

    /** {@link AnchorIndex#forgetShared} leert den Vorrat: die nächste
     *  Engine über den Regelsatz baut ihren Index neu, und dieser wird
     *  behalten (Rust {@code forgotten_indices_are_built_again}). */
    @Test
    void vergesseneIndizesWerdenNeuGebaut() {
        Taken first = indexOf("forget-a");
        assertTrue(AnchorIndex.keeps(first.rules()));
        AnchorIndex.forgetShared();
        assertFalse(AnchorIndex.keeps(first.rules()), "von diesem Regelsatz ist nichts behalten");
        assertEquals(0, AnchorIndex.kept());
        Taken again = indexOf("forget-a");
        assertTrue(AnchorIndex.keeps(again.rules()), "neu gebaut und behalten");
        assertNotSame(first.index(), again.index(), "neu gebaut");
    }

    /** Der Prozess behält die Indizes der letzten {@code KEPT}
     *  Regelsätze (Rust {@code the_kept_indices_are_bounded}). */
    @Test
    void derVorratIstBegrenzt() {
        assertEquals(32, AnchorIndex.KEPT);
        Taken oldest = indexOf("bounded-0");
        assertTrue(AnchorIndex.keeps(oldest.rules()));
        Taken last = null;
        for (int i = 1; i <= AnchorIndex.KEPT; i++) last = indexOf("bounded-" + i);
        assertTrue(AnchorIndex.kept() <= AnchorIndex.KEPT);
        assertTrue(AnchorIndex.keeps(last.rules()), "der zuletzt genutzte Regelsatz ist behalten");
        assertFalse(AnchorIndex.keeps(oldest.rules()), "der verdrängte nicht");
    }

    /** Eine Engine mit einem Index des Hosts nimmt diesen und nutzt den
     *  Vorrat nicht. */
    @Test
    void indexDesHostsGehtAmVorratVorbei() {
        AnchorIndex.forgetShared();
        Graph g = new Graph();
        Id f = g.addBaseline("file", "F");
        List<Rule> rules = Rules.load(rulesJson("host"), g);
        AnchorIndex host = AnchorIndex.build(rules);
        Engine e = new Engine(rules, host);
        e.elementsAdded(g, Map.of(), List.of(f));
        assertSame(host, e.anchorIndex());
        assertFalse(AnchorIndex.keeps(rules), "die Engine hat den Vorrat genutzt");
        assertEquals(0, AnchorIndex.kept());
    }

    /**
     * Acht Threads laden denselben Regelsatz je in einen eigenen Graphen,
     * starten gleichzeitig eine Engine über einen leeren Vorrat und fahren
     * die Kaskade. Wer den Index nicht findet, baut ihn, und nimmt danach
     * den Index, den der Vorrat dann hält: alle Engines lesen einen Index.
     * Jede liefert Schritte, Knoten und Fingerprint der Engine ohne Index.
     */
    @Test
    void achtThreadsBauenGleichzeitig() throws Exception {
        Langpack.Spec spec = new Langpack.Spec(50, 300);
        String plain = run(spec, false).result();
        AnchorIndex.forgetShared();
        int threads = 8;
        CyclicBarrier start = new CyclicBarrier(threads);
        ExecutorService pool = Executors.newFixedThreadPool(threads);
        try {
            List<Future<Ran>> runs = new ArrayList<>();
            for (int t = 0; t < threads; t++) {
                Callable<Ran> task = () -> {
                    Langpack.Model m = new Langpack.Model(spec);
                    List<Rule> rules = Rules.load(Langpack.rulesJson(spec.kinds()), m.g);
                    start.await(30, TimeUnit.SECONDS);
                    return run(m, rules, true);
                };
                runs.add(pool.submit(task));
            }
            AnchorIndex index = null;
            for (Future<Ran> f : runs) {
                Ran r = f.get(120, TimeUnit.SECONDS);
                assertEquals(plain, r.result(), "Schritte, angewandte Schritte, Knoten, Fingerprint");
                assertNotNull(r.index());
                if (index == null) index = r.index();
                assertSame(index, r.index(), "ein Index für alle Engines");
            }
        } finally {
            pool.shutdownNow();
        }
    }

    /** Ergebnis eines Laufs und der Index der Engine. */
    private record Ran(String result, AnchorIndex index) {}

    private static Ran run(Langpack.Spec spec, boolean useIndex) {
        Langpack.Model m = new Langpack.Model(spec);
        return run(m, Rules.load(Langpack.rulesJson(spec.kinds()), m.g), useIndex);
    }

    /** Das Modell als ein Add-Delta, dann bis zur Sättigung. */
    private static Ran run(Langpack.Model m, List<Rule> rules, boolean useIndex) {
        Engine e = new Engine(rules, useIndex);
        e.elementsAdded(m.g, m.vs, m.all);
        long steps = 0;
        while (e.step(m.g, m.vs) != null) steps++;
        String result = steps + " " + e.cascade.size() + " " + m.g.allNodes().size() + " "
                + Langpack.fingerprint(m.g, e, m.vs);
        return new Ran(result, e.anchorIndex());
    }
}
