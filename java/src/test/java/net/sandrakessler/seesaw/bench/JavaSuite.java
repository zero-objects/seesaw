package net.sandrakessler.seesaw.bench;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.function.Function;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.engine.Langpack;

/**
 * Messstrecke der Java-Engine, Gegenstück zur Rust-Suite
 * ({@code benches/suite/main.rs}). Hauptprogramm, kein JUnit:
 *
 * <pre>
 * JavaSuite list
 * JavaSuite run &lt;zelle&gt; [--label L] [--reps R] [--warmup W] [--tsv DATEI]
 * </pre>
 *
 * <p>Eine Zelle ist ein Workload mit einer Operation in einer Größe. Ein
 * Aufruf von {@code run} fährt eine Zelle: {@code W} Läufe zum Aufwärmen
 * der JVM, dann {@code R} gemessene Läufe, alle im selben Prozess. Vor
 * jedem Lauf ein voller GC, außerhalb der Messstrecke. Jeder Lauf baut
 * Modell, Regeln und Engine neu; gemessen wird nur die Strecke der Zelle
 * ({@link Section}). Alle Läufe, auch die zum Aufwärmen, müssen
 * dieselben Schritte, angewandten Schritte, Knoten, denselben Fingerprint
 * und dieselbe Zahl der Suchen liefern, sonst Abbruch.
 *
 * <p>Die Zeile geht nach stdout und mit {@code --tsv} ans Ende der Datei.
 * Spalten (Tab): Label, Zelle, gemessene Läufe, Thread-CPU min und
 * median (ms), Wall min (ms), Heap nach dem ersten gemessenen Lauf (MB,
 * belegt nach vollem GC, siehe {@link Section#heapBytes}), Schritte,
 * angewandte Schritte, Knoten, Fingerprint, dann {@code alloc=} (Bytes,
 * die der Thread in der Strecke alloziert), {@code gc_ms=} und
 * {@code gc_n=} (GC-Zeit und -Läufe in der Strecke), je das Minimum
 * über die gemessenen Läufe, {@code searches=} (gestartete Suchen der
 * Engine im ganzen Lauf, auch außerhalb der Strecke; -1, wo der Stand
 * den Zähler nicht hat) und {@code warmup=}.
 *
 * <p>Die Zellen haben die Namen der Rust-Suite, Schritte, angewandte
 * Schritte, Knoten und Fingerprint müssen gleich den Rust-Zellen sein
 * ({@code java/bench/jgate.sh fp}). Ob die Engine über den
 * {@code AnchorIndex} sucht, entscheidet
 * {@link Engine#defaultUseIndex()} (Systemeigenschaft
 * {@code seesaw.anchorIndex}).
 */
public final class JavaSuite {
    private JavaSuite() {}

    record Cell(String name, String about, Function<Section, Langpack.Outcome> run) {}

    /**
     * Die Zellen der Rust-Messreihe {@code 2026-11-15-auswahl}: die großen
     * ({@code k700/n2000}, {@code k700/n20000}, {@code f20000}) für die
     * Messung, die kleinen ({@code k50/n2000}, {@code f2000}) für den
     * Vergleich der Fingerprints. Dazu die Zellen mit mehreren Engines in
     * einem Prozess ({@code langpack/engines/k700/n200/e34},
     * {@code langpack/index/k700/e100}). {@code delta-change-same} und
     * {@code langpack/engines/k700/n2000/e10} (nicht in der Messreihe
     * {@code 2026-11-15-auswahl}) hat die Java-Seite nicht. Dazu je
     * Protokoll eines Hosts im Verzeichnis
     * {@code SEESAW_PDE_DUMP} die Zellen {@code pde/load/<name>},
     * {@code pde/graph/<name>}, {@code pde/open/<name>} und für
     * {@code <name>-edit} {@code pde/edit/<name>} ({@link PdeReplay});
     * ohne die Variable keine.
     */
    static List<Cell> cells() {
        List<Cell> out = new ArrayList<>();
        boolean index = Engine.defaultUseIndex();
        Langpack.Spec small = new Langpack.Spec(50, 2000);
        Langpack.Spec mid = new Langpack.Spec(700, 2000);
        Langpack.Spec large = new Langpack.Spec(700, 20000);
        for (Langpack.Spec spec : List.of(small, mid, large)) {
            String size = "k" + spec.kinds() + "/n" + spec.nodes();
            if (spec != large) {
                out.add(new Cell("langpack/seed/" + size, "Batch durch seed",
                        s -> Langpack.seed(spec, index, s)));
            }
            out.add(new Cell("langpack/add/" + size, "ganzes Modell als ein Add-Delta",
                    s -> Langpack.add(spec, index, s)));
            if (spec != large) {
                out.add(new Cell("langpack/backward/" + size, "Zielseite als ein Add-Delta",
                        s -> Langpack.backward(spec, index, s)));
            }
            if (spec != mid) {
                out.add(new Cell("langpack/delta-add/" + size + "/d200",
                        "200 neue Knoten, eine Welle je Knoten",
                        s -> Langpack.deltaAdd(spec, 200, index, s)));
                out.add(new Cell("langpack/delta-change/" + size + "/d200",
                        "200 geänderte Blattwerte, eine Welle je Änderung",
                        s -> Langpack.deltaChange(spec, 200, index, s)));
                out.add(new Cell("langpack/delta-delete/" + size + "/d200",
                        "200 gelöschte Knoten, eine Welle je Löschung",
                        s -> Langpack.deltaDelete(spec, 200, index, s)));
                out.add(new Cell("langpack/delta-delete/" + size + "/d2000",
                        "2000 gelöschte Knoten, eine Welle je Löschung",
                        s -> Langpack.deltaDelete(spec, 2000, index, s)));
            }
        }
        Langpack.Spec batches = new Langpack.Spec(700, 200);
        out.add(new Cell("langpack/engines/k700/n200/e34",
                "Läufe nacheinander, Modell, Regeln und Engine je Lauf neu",
                s -> Langpack.engines(batches, 34, index, s)));
        Langpack.Spec rulesOnly = new Langpack.Spec(700, 0);
        out.add(new Cell("langpack/index/k700/e100",
                "100 Engines über neu geladene Regeln, je eine leere Expansion",
                s -> Langpack.indexOnly(rulesOnly, 100, index, s)));
        for (int families : new int[] {2000, 20000}) {
            String size = "f" + families;
            out.add(new Cell("f2p/batch/" + size, "Batch durch run (Seed und Schritte)",
                    s -> F2p.batch(families, s)));
            out.add(new Cell("f2p/add/" + size, "ganzes Modell als ein Add-Delta",
                    s -> F2p.add(families, s)));
            out.add(new Cell("f2p/delta-add/" + size + "/d1000",
                    "1000 neue Familien, eine Welle je Familie",
                    s -> F2p.deltaAdd(families, 1000, s)));
            out.add(new Cell("f2p/delta-delete/" + size + "/d1000",
                    "1000 gelöschte Mitglieder, eine Welle je Löschung",
                    s -> F2p.deltaDelete(families, 1000, s)));
        }
        for (PdeReplay.Trace t : PdeReplay.records()) {
            for (PdeReplay.Part part : List.of(PdeReplay.Part.LOAD, PdeReplay.Part.GRAPH,
                    PdeReplay.Part.OPEN)) {
                out.add(new Cell("pde/" + part.cell + "/" + t.name(), part.about,
                        s -> PdeReplay.run(t.path(), part, s)));
            }
            if (t.name().endsWith("-edit")) {
                PdeReplay.Part part = PdeReplay.Part.EDIT;
                out.add(new Cell("pde/" + part.cell + "/" + t.name(), part.about,
                        s -> PdeReplay.run(t.path(), part, s)));
            }
        }
        return out;
    }

    private static void usage() {
        System.err.println("usage: JavaSuite list | run <zelle> [--label L] [--reps R] [--warmup W]"
                + " [--tsv DATEI]");
        System.exit(2);
    }

    public static void main(String[] args) throws IOException {
        if (args.length == 0) usage();
        List<Cell> cells = cells();
        switch (args[0]) {
            case "list" -> {
                for (Cell c : cells) System.out.println(c.name() + "\t" + c.about());
            }
            case "run" -> {
                if (args.length < 2) usage();
                String name = args[1];
                String label = "-";
                int reps = 5;
                int warmup = 3;
                String tsv = null;
                for (int i = 2; i < args.length; i++) {
                    if (i + 1 >= args.length) usage();
                    String value = args[++i];
                    try {
                        switch (args[i - 1]) {
                            case "--label" -> label = value;
                            case "--reps" -> reps = Integer.parseInt(value);
                            case "--warmup" -> warmup = Integer.parseInt(value);
                            case "--tsv" -> tsv = value;
                            default -> usage();
                        }
                    } catch (NumberFormatException ex) {
                        usage();
                    }
                }
                if (reps < 1 || warmup < 0) usage();
                Cell cell = null;
                for (Cell c : cells) if (c.name().equals(name)) cell = c;
                if (cell == null) {
                    System.err.println("unbekannte Zelle " + name);
                    System.exit(2);
                }
                String line = run(cell, warmup, reps, label);
                System.out.println(line);
                if (tsv != null) {
                    Files.writeString(Path.of(tsv), line + "\n", StandardCharsets.UTF_8,
                            StandardOpenOption.CREATE, StandardOpenOption.APPEND);
                }
            }
            default -> usage();
        }
    }

    private static String result(Langpack.Outcome o) {
        return o.steps() + " " + o.applied() + " " + o.nodes() + " " + o.fingerprint() + " "
                + o.searches();
    }

    private static String ms(long nanos) {
        return String.format(Locale.ROOT, "%.1f", nanos / 1e6);
    }

    static String run(Cell cell, int warmup, int reps, String label) {
        Langpack.Outcome first = null;
        long[] cpu = new long[reps];
        long wall = Long.MAX_VALUE;
        long alloc = Long.MAX_VALUE;
        long gcMillis = Long.MAX_VALUE;
        long gcCount = Long.MAX_VALUE;
        long heap = -1;
        for (int i = 0; i < warmup + reps; i++) {
            System.gc();
            Section s = new Section(i == warmup);
            Langpack.Outcome o = cell.run().apply(s);
            if (first == null) {
                first = o;
            } else if (!result(o).equals(result(first))) {
                throw new IllegalStateException(cell.name() + ": Lauf " + i + " liefert "
                        + result(o) + ", Lauf 0 " + result(first)
                        + " (Schritte, angewandt, Knoten, Fingerprint, Suchen)");
            }
            if (i < warmup) continue;
            cpu[i - warmup] = s.cpuNanos();
            wall = Math.min(wall, s.wallNanos());
            alloc = Math.min(alloc, s.allocBytes());
            gcMillis = Math.min(gcMillis, s.gcMillis());
            gcCount = Math.min(gcCount, s.gcCount());
            if (i == warmup) heap = s.heapBytes();
        }
        Arrays.sort(cpu);
        return String.join("\t", List.of(
                label,
                cell.name(),
                Integer.toString(reps),
                ms(cpu[0]),
                ms(cpu[reps / 2]),
                ms(wall),
                String.format(Locale.ROOT, "%.0f", heap / (1024.0 * 1024.0)),
                Long.toString(first.steps()),
                Long.toString(first.applied()),
                Long.toString(first.nodes()),
                first.fingerprint(),
                "alloc=" + alloc,
                "gc_ms=" + gcMillis,
                "gc_n=" + gcCount,
                "searches=" + first.searches(),
                "warmup=" + warmup));
    }
}
