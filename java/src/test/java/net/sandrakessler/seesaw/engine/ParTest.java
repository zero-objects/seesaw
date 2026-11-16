package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.bench.F2p;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Der Seed mit seinen Suchen auf mehreren Threads
 * ({@link Engine#seedWithThreads}, Rust {@code engine::par}): die Engine
 * traegt ein, was {@link Engine#seed} eintraegt, in derselben Folge, und
 * faehrt danach dieselbe Kaskade. Verglichen werden der Zustand der
 * Engine nach dem Seed (Match-Speicher in seiner Folge, Todo, angewandte
 * Schluessel, {@code byKey}) und der Graph ({@link Worlds#dump}), die
 * Zaehler der Suchen, die Rueckgaben von {@code step} bis zur Saettigung
 * und Graph, Kaskade und Zustand danach.
 */
final class ParTest {
    private static final Langpack.Spec LANG = new Langpack.Spec(50, 2000);
    private static final int FAMILIES = 1000;

    /** Graph, Werte, alle Baseline-Knoten und die Regeln eines Modells. */
    private record Model(Graph g, Map<Id, String> vals, List<Id> all, List<Rule> rules) {}

    private static Model langpack(Langpack.Spec spec) {
        Langpack.Model m = new Langpack.Model(spec);
        List<Rule> rules = Rules.load(Langpack.rulesJson(spec.kinds()), m.g);
        return new Model(m.g, m.vs, m.all, rules);
    }

    private static Model f2p(int families) {
        F2p.Model m = new F2p.Model(families);
        List<Rule> rules = Rules.load(F2p.rulesJson(), m.g);
        return new Model(m.g, m.vs, m.all, rules);
    }

    /** Das Modell nach einer ersten Engine, die es ganz uebersetzt hat:
     *  ein Seed darauf findet die Matches jeder Regel. */
    private static Model translated(Model m) {
        Engine first = new Engine(m.rules(), true);
        first.elementsAdded(m.g(), m.vals(), m.all());
        while (first.step(m.g(), m.vals()) != null) { }
        return m;
    }

    /** Die Zaehler der Suchen (Task 11) und der Seeds ueber den Index. */
    private static String counters(Engine e) {
        return "searches=" + e.searches + " seedsByIndex=" + e.seedsByIndex + " anchored="
                + e.anchoredSearches + " seedSearches=" + e.seedSearches + " visits="
                + e.walk.visits;
    }

    /** Eine Engine fuer den Weg {@code byIndex}: mit Index und Schwelle 0
     *  geht der Seed ueber den Index, ohne Index ist er der Scan. */
    private static Engine engine(List<Rule> rules, boolean byIndex) {
        Engine e = new Engine(rules, byIndex);
        if (byIndex) e.seedFactor = 0;
        return e;
    }

    /**
     * Seed auf {@code threads} Threads (0: {@link Engine#seed}), dann
     * Schritte bis zur Saettigung. Liefert den Abzug nach dem Seed, die
     * Zaehler, die Rueckgaben von {@code step} und den Abzug danach.
     */
    private static List<String> run(Supplier<Model> model, boolean again, boolean byIndex,
            int threads) {
        Model m = model.get();
        if (again) translated(m);
        Engine e = engine(m.rules(), byIndex);
        if (threads == 0) {
            e.seed(m.g(), m.vals());
        } else {
            e.seedWithThreads(m.g(), m.vals(), threads);
        }
        assertEquals(byIndex ? 1 : 0, e.seedsByIndex, "Weg des Seeds");
        List<String> out = new ArrayList<>();
        out.add(Worlds.dump(m.g(), m.vals(), e));
        out.add(counters(e));
        out.add(Worlds.steps(e, m.g(), m.vals(), Integer.MAX_VALUE));
        out.add(Worlds.dump(m.g(), m.vals(), e));
        return out;
    }

    private static final String[] PARTS = {"nach dem Seed", "Zaehler", "Schritte",
        "nach den Schritten"};

    private static String line(String[] lines, int i) {
        if (i >= lines.length) return "(Ende)";
        String l = lines[i];
        return l.length() > 300 ? l.substring(0, 300) + " ..." : l;
    }

    /** Gleiche Abzuege, sonst die erste abweichende Zeile. */
    private static void same(String what, List<String> expected, List<String> actual) {
        for (int i = 0; i < expected.size(); i++) {
            String a = expected.get(i);
            String b = actual.get(i);
            if (a.equals(b)) continue;
            String[] x = a.split("\n", -1);
            String[] y = b.split("\n", -1);
            int k = 0;
            while (k < x.length && k < y.length && x[k].equals(y[k])) k++;
            fail(what + ", " + PARTS[i] + ": erste Abweichung in Zeile " + (k + 1) + ", erwartet <"
                    + line(x, k) + ">, war <" + line(y, k) + ">");
        }
    }

    private static int count(String dump, String prefix) {
        int n = 0;
        for (String l : dump.split("\n")) if (l.startsWith(prefix)) n++;
        return n;
    }

    /** Je Modell vor und nach der Uebersetzung, je Weg: 1, 4 und 8
     *  Threads gegen {@link Engine#seed}. */
    private static void againstSeed(String name, Supplier<Model> model) {
        for (boolean again : new boolean[] {false, true}) {
            for (boolean byIndex : new boolean[] {true, false}) {
                List<String> expected = run(model, again, byIndex, 0);
                String what = name + (again ? " uebersetzt" : " roh")
                        + (byIndex ? ", Index" : ", Scan");
                // Auf dem rohen Sprachpack findet der Seed nur, was keine
                // uebersetzten Eltern braucht, den Rest die Expansion der
                // Schritte. Auf dem uebersetzten Modell die Matches jeder
                // Regel.
                assertTrue(count(expected.get(0), "match ") > (again ? 1000 : 0),
                        what + ": zu wenig Matches");
                assertTrue(expected.get(2).contains("a") || again, what + ": nichts angewandt");
                for (int threads : new int[] {1, 4, 8}) {
                    same(what + ", " + threads + " Threads", expected,
                            run(model, again, byIndex, threads));
                }
            }
        }
    }

    @Test
    void langpackWieSeed() {
        againstSeed("langpack", () -> langpack(LANG));
    }

    @Test
    void f2pWieSeed() {
        againstSeed("f2p", () -> f2p(FAMILIES));
    }

    /**
     * Fuer jeden Zuschnitt der Arbeit (Threads, Stueckgroesse) traegt der
     * verteilte Seed ueber den Index die Matches von {@link Engine#seed}
     * in ihrer Folge ein, auch mit einem Thread und mit Regeln, die die
     * Welle nicht zulaesst (Rust
     * {@code the_spread_index_seed_keeps_matches_and_order}). Der Scan
     * ebenso fuer 2, 3 und 16 Threads (Rust
     * {@code the_spread_scan_keeps_matches_and_order}).
     */
    @Test
    void jederZuschnittWieSeed() {
        Model m = translated(langpack(new Langpack.Spec(50, 300)));
        int[][] cuts = {{2, 1}, {3, 7}, {4, 64}, {8, 100_000}, {1, 5}};
        for (boolean forwardOnly : new boolean[] {false, true}) {
            for (boolean byIndex : new boolean[] {true, false}) {
                Engine ref = engine(m.rules(), byIndex);
                if (forwardOnly) ref.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
                ref.seed(m.g(), m.vals());
                List<String> expected = List.of(Worlds.dump(m.g(), m.vals(), ref), counters(ref));
                String what = (byIndex ? "Index" : "Scan") + (forwardOnly ? ", nur vorwaerts" : "");
                assertTrue(count(expected.get(0), "match ") > 300, what + ": zu wenig Matches");
                int[][] runs = byIndex ? cuts : new int[][] {{2, 1}, {3, 1}, {16, 1}};
                for (int[] cut : runs) {
                    Engine e = engine(m.rules(), byIndex);
                    if (forwardOnly) e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
                    e.seedSpread(m.g(), m.vals(), cut[0], cut[1]);
                    same(what + ", " + cut[0] + " Threads, Stuecke zu " + cut[1], expected,
                            List.of(Worlds.dump(m.g(), m.vals(), e), counters(e)));
                }
            }
        }
    }

    /** Eine Ausnahme in einem der Threads kommt beim Aufrufer an, und
     *  kein Thread laeuft danach weiter. */
    @Test
    void fehlerEinesThreadsKommtBeimAufruferAn() {
        Model m = translated(langpack(new Langpack.Spec(50, 300)));
        IllegalStateException boom = new IllegalStateException("Wert nicht lesbar");
        Thread caller = Thread.currentThread();
        Map<Id, String> vals = new HashMap<>(m.vals()) {
            @Override public String get(Object key) {
                if (Thread.currentThread() != caller) throw boom;
                return super.get(key);
            }
        };
        for (boolean byIndex : new boolean[] {true, false}) {
            Engine e = engine(m.rules(), byIndex);
            RuntimeException got = assertThrows(RuntimeException.class,
                    () -> e.seedWithThreads(m.g(), vals, 4));
            assertSame(boom, got, byIndex ? "Index" : "Scan");
            for (Thread t : Thread.getAllStackTraces().keySet()) {
                assertTrue(!t.getName().startsWith("seesaw-seed-"), "laeuft noch: " + t.getName());
            }
        }
    }
}
