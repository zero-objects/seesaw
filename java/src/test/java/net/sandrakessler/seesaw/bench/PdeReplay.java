package net.sandrakessler.seesaw.bench;

import java.io.BufferedOutputStream;
import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.PrintStream;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.DirectoryStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.TreeMap;
import java.util.function.Supplier;

import net.sandrakessler.seesaw.engine.AnchorIndex;
import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.engine.Langpack;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Workload {@code pde} der Rust-Suite ({@code benches/suite/pde.rs}): die
 * Aufrufe, die ein echter Host (zerodds-pde) an der öffentlichen
 * Schnittstelle von seesaw-tgg gemacht hat, gegen die Java-Engine
 * nachgefahren. Jedes Ergebnis wird gegen das Protokoll geprüft, bei der
 * ersten Abweichung bricht die Nachfahrt ab ({@link Mismatch}: Datei,
 * Zeilennummer, Aufruf, erwartet, bekommen). Damit ist eine Zelle
 * zugleich die Gleichheitsprüfung Java gegen Rust auf diesem Workload:
 * Schritte, angewandte Schritte, Knoten und Fingerprint müssen gleich
 * den Rust-Zellen sein.
 *
 * <p>Die Protokolle liegen in keinem Repository. {@code SEESAW_PDE_DUMP}
 * nennt das Verzeichnis, je Protokoll eine Datei {@code <name>.trace}.
 * Zellen je Protokoll wie in Rust:
 * <ul>
 * <li>{@code pde/load/<name>}: vom Anfang bis hinter die letzte
 *     Regeldatei, den letzten Index oder die letzte Engine vor dem ersten
 *     Aufruf an einer Engine.</li>
 * <li>{@code pde/graph/<name>}: von dort bis zur ersten Marke (oder zum
 *     Ende) die Aufrufe des Hosts an Graph und Wertespeicher.</li>
 * <li>{@code pde/open/<name>}: dieselbe Strecke, die Aufrufe an den
 *     Engines (die Wellen). Host und Engines wechseln sich dort ab, jede
 *     der beiden Zellen zählt die Stücke ihrer Art.</li>
 * <li>{@code pde/edit/<name>} für Protokolle {@code <name>-edit}: alles
 *     ab der ersten Marke.</li>
 * </ul>
 * Jede Zelle fährt das ganze Protokoll. Sie unterscheiden sich nur in
 * dem, was gemessen wird ({@link Section} mit {@code pause}/{@code resume}
 * wie {@code measure::Meter}). Schritte, Knoten und Fingerprint sind
 * also in allen Zellen eines Protokolls gleich.
 *
 * <h2>Format</h2>
 *
 * <p>Ein Aufruf je Zeile, Felder durch ein Leerzeichen getrennt.
 * Zeichenketten tragen {@code \s} für ein Leerzeichen, {@code \n},
 * {@code \r}, {@code \t}, {@code \\}. {@code \e} allein ist die leere
 * Zeichenkette. Eine Id ist {@code @<hex>}, wo sie zuerst vorkommt (sie
 * bekommt die nächste Nummer ab 0), danach {@code #<nummer>}. Die Regeln
 * sind in der Reihenfolge nummeriert, in der der Rust-Lader sie lieferte.
 * Eine Folge von Regeln ist eine Liste von Läufen {@code <erste>+<zahl>},
 * durch Kommas getrennt, {@code -} wenn leer. {@code G}, {@code V},
 * {@code I}, {@code E} sind die Nummern eines Graphen, eines
 * Wertespeichers, eines Index und einer Engine. {@code C N} am Ende eines
 * Aufrufs an einer Engine sind die Länge ihrer Kaskade und die Zahl der
 * Knoten des Graphen nach dem Aufruf.
 *
 * <h2>Zuordnung zur Java-API</h2>
 *
 * <p>Die Java-API weicht in Namen und Typen von der Rust-API ab. Die
 * Nachfahrt nutzt nur öffentliche API der Hauptklassen (sie läuft gegen
 * die Haupt-JARs älterer Stände, {@code java/bench/jsuite.sh}).
 *
 * <table>
 * <caption>Befehl, Rust-Aufruf, Java-Aufruf, Prüfung</caption>
 * <tr><th>Befehl</th><th>Rust</th><th>Java</th><th>geprüft</th></tr>
 * <tr><td>{@code gnew G}</td><td>{@code Graph::new}</td>
 *     <td>{@code new Graph()}</td><td>-</td></tr>
 * <tr><td>{@code gclone G G'}</td><td>{@code G.clone()}</td>
 *     <td>neuer Graph, Typen in der Reihenfolge von
 *     {@code G.typesByName} mit {@code intern}. Java hat kein
 *     {@code clone}, und Knoten und Verbindungen lassen sich über die
 *     öffentliche API nicht in derselben inneren Reihenfolge
 *     ({@code byType}) kopieren. Hat {@code G} Knoten oder
 *     Verbindungen, bricht die Nachfahrt ab ({@link Unsupported}).</td>
 *     <td>Typnummern gleich</td></tr>
 * <tr><td>{@code intern G name}</td><td>{@code G.types.intern(name)}</td>
 *     <td>{@code G.intern(name)}</td><td>-</td></tr>
 * <tr><td>{@code base G type external id}</td>
 *     <td>{@code G.add_baseline(external, type)}</td>
 *     <td>{@code G.addBaseline(external, type)}</td><td>Id</td></tr>
 * <tr><td>{@code conn G a b Status id|-}</td>
 *     <td>{@code G.connect(a, b, Status)}</td>
 *     <td>{@code G.connect(a, b, St)}, {@code null} für {@code -}</td>
 *     <td>Id</td></tr>
 * <tr><td>{@code nstat G id Status 0|1}</td>
 *     <td>{@code G.set_node_status(id, Status)}, {@code true} wenn der
 *     Knoten existiert</td>
 *     <td>{@code G.setNodeStatus(id, St)} gibt nichts zurück, die
 *     Rückgabe ist {@code G.node(id) != null} vor dem Aufruf</td>
 *     <td>Rückgabe</td></tr>
 * <tr><td>{@code cstat G id Status 0|1}</td>
 *     <td>{@code G.set_connection_status(id, Status)}</td>
 *     <td>{@code G.setConnectionStatus(id, St)}, Rückgabe
 *     {@code G.conn(id) != null} vor dem Aufruf</td>
 *     <td>Rückgabe</td></tr>
 * <tr><td>{@code vnew V}</td><td>{@code ValueStore::default}</td>
 *     <td>{@code new HashMap<Id, String>()}</td><td>-</td></tr>
 * <tr><td>{@code vclone V V'}</td><td>{@code V.clone()}</td>
 *     <td>{@code new HashMap<>(V)}</td><td>-</td></tr>
 * <tr><td>{@code val V id value}</td><td>{@code V.insert(id, value)}</td>
 *     <td>{@code V.put(id, value)}</td><td>-</td></tr>
 * <tr><td>{@code load G first count json}</td>
 *     <td>{@code rules::load_file(json, G)}</td>
 *     <td>{@code Rules.load(json, G)}, angehängt an die Liste aller
 *     Regeln</td>
 *     <td>Zahl der Regeln und erste Nummer</td></tr>
 * <tr><td>{@code ibuild I rules}</td><td>{@code AnchorIndex::build(rules)}</td>
 *     <td>{@code AnchorIndex.build(rules)}</td><td>-</td></tr>
 * <tr><td>{@code enew E rules}</td><td>{@code Engine::new(rules)}</td>
 *     <td>{@code new Engine(rules)} (Suchweg nach
 *     {@code seesaw.anchorIndex})</td><td>-</td></tr>
 * <tr><td>{@code enewidx E rules I}</td>
 *     <td>{@code Engine::with_anchor_index(rules, I)},
 *     {@code I = 0}: {@code AnchorIndex::default()}</td>
 *     <td>{@code new Engine(rules, I)}, {@code I = 0}:
 *     {@code new Engine(rules, (AnchorIndex) null)}. Beide Seiten bauen
 *     dann den eigenen Index (der leere Index passt in Rust zu keinen
 *     Regeln).</td><td>-</td></tr>
 * <tr><td>{@code contra E 0|1}</td><td>{@code E.saw_contradiction = ..}</td>
 *     <td>{@code E.sawContradiction = ..}</td><td>-</td></tr>
 * <tr><td>{@code active E none|bits}</td><td>{@code E.active = ..}</td>
 *     <td>{@code none}: nichts. Die Java-Engine hat kein Feld
 *     {@code active}, jede Regel ist aktiv wie bei
 *     {@code active = None}. Eine Maske bricht beim Lesen ab
 *     ({@link Unsupported}).</td><td>-</td></tr>
 * <tr><td>{@code admitd E letters}</td><td>{@code E.admit_delta(..)}</td>
 *     <td>{@code E.admitDelta(List<DeltaDomain>)}</td><td>-</td></tr>
 * <tr><td>{@code admiti E}</td><td>{@code E.admit_initial()}</td>
 *     <td>{@code E.admitInitial()}</td><td>-</td></tr>
 * <tr><td>{@code added E G V n id.. C N}</td>
 *     <td>{@code E.elements_added(G, V, ids)}</td>
 *     <td>{@code E.elementsAdded(G, V, List<Id>)}</td><td>C N</td></tr>
 * <tr><td>{@code step E G V T|F|N C N}</td><td>{@code E.step(G, V)}</td>
 *     <td>{@code E.step(G, V)}: {@code TRUE}, {@code FALSE},
 *     {@code null}</td><td>Rückgabe, C N</td></tr>
 * <tr><td>{@code changed E G id C N}</td>
 *     <td>{@code E.element_changed(G, id)}</td>
 *     <td>{@code E.elementChanged(G, id)}</td><td>C N</td></tr>
 * <tr><td>{@code deleted E G id C N}</td>
 *     <td>{@code E.element_deleted(G, id)}</td>
 *     <td>{@code E.elementDeleted(G, id)}</td><td>C N</td></tr>
 * <tr><td>{@code consol E G C N}</td><td>{@code E.consolidate(G)}</td>
 *     <td>{@code E.consolidate(G)} (Rückgabe {@code int} ungenutzt)</td>
 *     <td>C N</td></tr>
 * <tr><td>{@code linkrm E G a b C N}</td><td>{@code E.link_removed(G, a, b)}</td>
 *     <td>{@code E.linkRemoved(G, a, b)}</td><td>C N</td></tr>
 * <tr><td>{@code linkadd E G V a b C N}</td>
 *     <td>{@code E.link_added(G, V, a, b)}</td>
 *     <td>{@code E.linkAdded(G, V, a, b)}</td><td>C N</td></tr>
 * <tr><td>{@code seed E G V C N}</td><td>{@code E.seed(G, V)}</td>
 *     <td>{@code E.seed(G, V)}</td><td>C N</td></tr>
 * <tr><td>{@code removed E id}</td><td>{@code E.element_removed(id)}</td>
 *     <td>{@code E.elementRemoved(id)}</td><td>-</td></tr>
 * <tr><td>{@code mark name}</td><td>Marke des Hosts</td>
 *     <td>Grenze der Zelle {@code edit}</td><td>-</td></tr>
 * <tr><td>{@code unsupported what}</td><td>Abbruch</td>
 *     <td>Abbruch beim Lesen ({@link Unsupported})</td><td>-</td></tr>
 * <tr><td>{@code retract E G id C N}</td><td>Abbruch
 *     ({@code retract_for}, veraltet)</td>
 *     <td>Abbruch beim Lesen ({@link Unsupported})</td><td>-</td></tr>
 * </table>
 *
 * <p>Kaskade ist {@code E.cascade.size()}, Knoten {@code G.nodeCount()}
 * (alle Knoten gleich welchen Status, wie {@code node_count}).
 *
 * <p>Teilfolgen von Regeln sind Kopien ({@code ArrayList}) aus der Liste
 * aller Regeln, keine {@code subList}-Sichten: eine Sicht auf die Liste,
 * die das nächste {@code load} verlängert, würfe danach bei jedem Zugriff
 * {@code ConcurrentModificationException}. Rust reicht Ausschnitte
 * desselben Speichers weiter. Die Regeln sind in beiden Fällen dieselben
 * Objekte, {@code AnchorIndex.fits} vergleicht den Inhalt.
 *
 * <p>Fingerprint wie {@code fingerprint::of_all}: Graph und Wertespeicher
 * des letzten Aufrufs an einer Engine mit Wertespeicher, dann die Kaskade
 * jeder Engine in der Reihenfolge ihrer Konstruktion
 * ({@link Langpack#fingerprint(Graph, List, Map)}).
 *
 * <h2>Ohne Abbruch</h2>
 *
 * <p>{@link #full} (Hauptprogramm {@code PdeReplay voll <protokoll>})
 * fährt das ganze Protokoll, ohne bei einer Abweichung abzubrechen, und
 * schreibt je Aufruf an einer Engine eine Zeile mit dem, was die Engine
 * zurückgab. Zwei Stände der Hauptklassen lassen sich so auch über den
 * Teil eines Protokolls vergleichen, der hinter der ersten Abweichung vom
 * Protokoll liegt ({@code java/bench/jgate.sh pde-voll}).
 */
public final class PdeReplay {
    private PdeReplay() {}

    /** Umgebungsvariable mit dem Verzeichnis der Protokolle. */
    public static final String DUMP = "SEESAW_PDE_DUMP";

    /** Welchen Teil eines Protokolls eine Zelle misst. */
    public enum Part {
        LOAD("load", "Host-Protokoll: Regeldateien geladen, Indizes gebaut"),
        GRAPH("graph", "Host-Protokoll: Modell in den Graphen gelegt"),
        OPEN("open", "Host-Protokoll: Wellen der Engines"),
        EDIT("edit", "Host-Protokoll: Wellen nach der ersten Marke (Bearbeitungen)");

        public final String cell;
        public final String about;

        Part(String cell, String about) {
            this.cell = cell;
            this.about = about;
        }
    }

    /** Ein Protokoll: Name (Datei ohne {@code .trace}) und Pfad. */
    public record Trace(String name, Path path) {}

    /** Die Protokolle im Verzeichnis {@code SEESAW_PDE_DUMP}, nach Namen
     *  sortiert. Leer, wenn die Variable nicht gesetzt ist. */
    public static List<Trace> records() {
        String dir = System.getenv(DUMP);
        if (dir == null) return List.of();
        List<Trace> out = new ArrayList<>();
        try (DirectoryStream<Path> ds = Files.newDirectoryStream(Path.of(dir), "*.trace")) {
            for (Path p : ds) {
                String f = p.getFileName().toString();
                out.add(new Trace(f.substring(0, f.length() - ".trace".length()), p));
            }
        } catch (IOException ex) {
            throw new UncheckedIOException(DUMP + "=" + dir, ex);
        }
        out.sort((a, b) -> a.name().compareTo(b.name()));
        return out;
    }

    /** Eine Abweichung der Engine vom Protokoll. */
    public static final class Mismatch extends IllegalStateException {
        public final int line;

        Mismatch(String message, int line) {
            super(message);
            this.line = line;
        }
    }

    /** Ein Aufruf des Protokolls, den die Nachfahrt nicht abbilden kann. */
    public static final class Unsupported extends IllegalStateException {
        public final int line;

        Unsupported(String message, int line) {
            super(message);
            this.line = line;
        }
    }

    // ── Aufrufe ──

    private sealed interface Op {}

    private record GraphNew(int g) implements Op {}

    private record GraphClone(int from, int to) implements Op {}

    private record Intern(int g, String name) implements Op {}

    private record Base(int g, String typ, String external, Id id) implements Op {}

    /** {@code id == null}: {@code connect} lieferte {@code None}. */
    private record Connect(int g, Id a, Id b, St status, Id id) implements Op {}

    private record NodeStatus(int g, Id id, St status, boolean done) implements Op {}

    private record ConnectionStatus(int g, Id id, St status, boolean done) implements Op {}

    private record StoreNew(int v) implements Op {}

    private record StoreClone(int from, int to) implements Op {}

    private record Value(int v, Id id, String value) implements Op {}

    private record Load(int g, int first, int count, String json) implements Op {}

    /** {@code runs}: Paare (erste, Zahl). */
    private record IndexBuild(int i, int[] runs) implements Op {}

    /** {@code index < 0}: {@code enew}. */
    private record EngineNew(int e, int[] runs, int index) implements Op {}

    private record Contradiction(int e, boolean value) implements Op {}

    /** {@code active E none}. */
    private record ActiveNone(int e) implements Op {}

    private record AdmitDelta(int e, List<Engine.DeltaDomain> domains) implements Op {}

    private record AdmitInitial(int e) implements Op {}

    private record Mark() implements Op {}

    private record Added(int e, int g, int v, Id[] ids, long cascade, long nodes) implements Op {}

    private record Step(int e, int g, int v, Boolean result, long cascade, long nodes)
            implements Op {}

    private record Changed(int e, int g, Id id, long cascade, long nodes) implements Op {}

    private record Deleted(int e, int g, Id id, long cascade, long nodes) implements Op {}

    private record Consolidate(int e, int g, long cascade, long nodes) implements Op {}

    private record LinkRemoved(int e, int g, Id a, Id b, long cascade, long nodes)
            implements Op {}

    private record LinkAdded(int e, int g, int v, Id a, Id b, long cascade, long nodes)
            implements Op {}

    private record Seed(int e, int g, int v, long cascade, long nodes) implements Op {}

    private record Removed(int e, Id id) implements Op {}

    /** Ein Aufruf an einer bestehenden Engine (nicht ihre Konstruktion). */
    private static boolean isWave(Op op) {
        return !(op instanceof GraphNew || op instanceof GraphClone || op instanceof Intern
                || op instanceof Base || op instanceof Connect || op instanceof NodeStatus
                || op instanceof ConnectionStatus || op instanceof StoreNew
                || op instanceof StoreClone || op instanceof Value || op instanceof Load
                || op instanceof IndexBuild || op instanceof EngineNew || op instanceof Mark);
    }

    /** Ein Aufruf des Hosts an einem Graphen oder Wertespeicher. */
    private static boolean isHost(Op op) {
        return op instanceof GraphNew || op instanceof GraphClone || op instanceof Intern
                || op instanceof Base || op instanceof Connect || op instanceof NodeStatus
                || op instanceof ConnectionStatus || op instanceof StoreNew
                || op instanceof StoreClone || op instanceof Value;
    }

    private static boolean isSetup(Op op) {
        return op instanceof Load || op instanceof IndexBuild || op instanceof EngineNew;
    }

    // ── Lesen ──

    /** Ein gelesenes Protokoll. Der Aufruf {@code ops[i]} steht in Zeile
     *  {@code i + 1}. */
    private static final class Record {
        final Path path;
        final List<Op> ops = new ArrayList<>();
        final List<Id> ids = new ArrayList<>();

        Record(Path path) { this.path = path; }

        /** Wo der Teil {@code load} endet und wo die Bearbeitungen
         *  beginnen: hinter der letzten Regeldatei, dem letzten Index oder
         *  der letzten Engine vor dem ersten Aufruf an einer Engine, und
         *  an der ersten Marke (ohne Marke das Ende). */
        int[] bounds() {
            int end = ops.size();
            int wave = end;
            for (int i = 0; i < end; i++) {
                if (isWave(ops.get(i))) {
                    wave = i;
                    break;
                }
            }
            int load = 0;
            for (int i = wave - 1; i >= 0; i--) {
                if (isSetup(ops.get(i))) {
                    load = i + 1;
                    break;
                }
            }
            int mark = end;
            for (int i = 0; i < end; i++) {
                if (ops.get(i) instanceof Mark) {
                    mark = i;
                    break;
                }
            }
            return new int[] {load, mark};
        }
    }

    /** Die Felder einer Zeile, mit Zeilennummer für die Meldungen. */
    private static final class Fields {
        final String[] f;
        final int line;
        final Record r;
        int at;

        Fields(String text, int line, Record r) {
            this.f = text.split(" ", -1);
            this.line = line;
            this.r = r;
        }

        IllegalStateException error(String what) {
            return new IllegalStateException(r.path + ":" + line + ": " + what);
        }

        String next() {
            if (at >= f.length) throw error("Feld fehlt");
            return f[at++];
        }

        void end() {
            if (at != f.length) throw error("überzähliges Feld " + f[at]);
        }

        /** Eine Zahl ohne Vorzeichen bis {@code u32::MAX}. */
        long num(String s) {
            if (s.isEmpty() || s.length() > 10) throw error("Zahl " + s);
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                if (c < '0' || c > '9') throw error("Zahl " + s);
            }
            long v = Long.parseLong(s);
            if (v > 0xFFFF_FFFFL) throw error("Zahl " + s);
            return v;
        }

        long num() { return num(next()); }

        /** Eine Nummer (Graph, Speicher, Index, Engine, Regel). */
        int ix() {
            String s = next();
            long v = num(s);
            if (v > Integer.MAX_VALUE) throw error("Nummer " + s);
            return (int) v;
        }

        boolean flag() {
            String s = next();
            return switch (s) {
                case "0" -> false;
                case "1" -> true;
                default -> throw error("0 oder 1 erwartet, nicht " + s);
            };
        }

        String text() { return unescape(next()); }

        String unescape(String s) {
            if (s.equals("\\e")) return "";
            if (s.indexOf('\\') < 0) return s;
            StringBuilder out = new StringBuilder(s.length());
            for (int i = 0; i < s.length(); i++) {
                char c = s.charAt(i);
                if (c != '\\') {
                    out.append(c);
                    continue;
                }
                if (++i >= s.length()) throw error("Escape am Ende in " + s);
                switch (s.charAt(i)) {
                    case 's' -> out.append(' ');
                    case 'n' -> out.append('\n');
                    case 'r' -> out.append('\r');
                    case 't' -> out.append('\t');
                    case '\\' -> out.append('\\');
                    default -> throw error("Escape \\" + s.charAt(i));
                }
            }
            return out.toString();
        }

        Id hex(String hex) {
            if (hex.length() != 64) throw error("Id " + hex);
            byte[] b = new byte[32];
            for (int i = 0; i < 32; i++) {
                int hi = Character.digit(hex.charAt(2 * i), 16);
                int lo = Character.digit(hex.charAt(2 * i + 1), 16);
                if (hi < 0 || lo < 0) throw error("Id " + hex);
                b[i] = (byte) (hi << 4 | lo);
            }
            return new Id(b);
        }

        /** Eine Id: {@code @<hex>} definiert sie, {@code #<nummer>}
         *  verweist auf sie. */
        Id id() {
            String t = next();
            if (t.startsWith("@")) {
                Id id = hex(t.substring(1));
                r.ids.add(id);
                return id;
            }
            if (t.startsWith("#")) {
                long n = num(t.substring(1));
                if (n >= r.ids.size()) throw error("Id " + t + " vor ihrer Definition");
                return r.ids.get((int) n);
            }
            throw error("Id " + t);
        }

        St status() {
            String s = next();
            return switch (s) {
                case "Solid" -> St.SOLID;
                case "Ghost" -> St.GHOST;
                case "TentativeTombstone" -> St.TENTATIVE_TOMBSTONE;
                case "Tombstone" -> St.TOMBSTONE;
                default -> throw error("Status " + s);
            };
        }

        int[] runs() {
            String s = next();
            if (s.equals("-")) return new int[0];
            String[] parts = s.split(",", -1);
            int[] out = new int[2 * parts.length];
            for (int i = 0; i < parts.length; i++) {
                int plus = parts[i].indexOf('+');
                if (plus < 0) throw error("Lauf " + parts[i]);
                long first = num(parts[i].substring(0, plus));
                long count = num(parts[i].substring(plus + 1));
                if (first + count > Integer.MAX_VALUE) throw error("Lauf " + parts[i]);
                out[2 * i] = (int) first;
                out[2 * i + 1] = (int) count;
            }
            return out;
        }
    }

    private static Record read(Path path) {
        Record r = new Record(path);
        Map<String, String> typeNames = new HashMap<>();
        try (BufferedReader in = Files.newBufferedReader(path, StandardCharsets.UTF_8)) {
            String text;
            int line = 0;
            while ((text = in.readLine()) != null) {
                line++;
                Fields f = new Fields(text, line, r);
                String kind = f.next();
                Op op = switch (kind) {
                    case "gnew" -> new GraphNew(f.ix());
                    case "gclone" -> new GraphClone(f.ix(), f.ix());
                    case "intern" -> new Intern(f.ix(), f.text());
                    case "base" -> {
                        int g = f.ix();
                        String typ = typeNames.computeIfAbsent(f.text(), t -> t);
                        String external = f.text();
                        yield new Base(g, typ, external, f.id());
                    }
                    case "conn" -> {
                        int g = f.ix();
                        Id a = f.id();
                        Id b = f.id();
                        St status = f.status();
                        Id id;
                        if (f.f.length > f.at && f.f[f.at].equals("-")) {
                            f.at++;
                            id = null;
                        } else {
                            id = f.id();
                        }
                        yield new Connect(g, a, b, status, id);
                    }
                    case "nstat" -> new NodeStatus(f.ix(), f.id(), f.status(), f.flag());
                    case "cstat" -> new ConnectionStatus(f.ix(), f.id(), f.status(), f.flag());
                    case "vnew" -> new StoreNew(f.ix());
                    case "vclone" -> new StoreClone(f.ix(), f.ix());
                    case "val" -> new Value(f.ix(), f.id(), f.text());
                    case "load" -> new Load(f.ix(), f.ix(), f.ix(), f.text());
                    case "ibuild" -> new IndexBuild(f.ix(), f.runs());
                    case "enew" -> new EngineNew(f.ix(), f.runs(), -1);
                    case "enewidx" -> new EngineNew(f.ix(), f.runs(), f.ix());
                    case "contra" -> new Contradiction(f.ix(), f.flag());
                    case "active" -> {
                        int e = f.ix();
                        String bits = f.next();
                        if (!bits.equals("none")) {
                            throw new Unsupported(path + ":" + line + ": active " + e + " "
                                    + bits + ": die Java-Engine hat keine Maske aktiver Regeln"
                                    + " (Rust Engine::active)", line);
                        }
                        yield new ActiveNone(e);
                    }
                    case "admitd" -> {
                        int e = f.ix();
                        String letters = f.text();
                        List<Engine.DeltaDomain> domains = new ArrayList<>(letters.length());
                        for (int i = 0; i < letters.length(); i++) {
                            domains.add(switch (letters.charAt(i)) {
                                case 'S' -> Engine.DeltaDomain.SOURCE;
                                case 'T' -> Engine.DeltaDomain.TARGET;
                                default -> throw f.error("Delta-Domäne " + letters.charAt(i));
                            });
                        }
                        yield new AdmitDelta(e, domains);
                    }
                    case "admiti" -> new AdmitInitial(f.ix());
                    case "mark" -> {
                        f.next();
                        yield new Mark();
                    }
                    case "added" -> {
                        int e = f.ix();
                        int g = f.ix();
                        int v = f.ix();
                        int n = f.ix();
                        Id[] ids = new Id[n];
                        for (int i = 0; i < n; i++) ids[i] = f.id();
                        yield new Added(e, g, v, ids, f.num(), f.num());
                    }
                    case "step" -> {
                        int e = f.ix();
                        int g = f.ix();
                        int v = f.ix();
                        String t = f.next();
                        Boolean result = switch (t) {
                            case "T" -> Boolean.TRUE;
                            case "F" -> Boolean.FALSE;
                            case "N" -> null;
                            default -> throw f.error("Ergebnis von step " + t);
                        };
                        yield new Step(e, g, v, result, f.num(), f.num());
                    }
                    case "changed" -> new Changed(f.ix(), f.ix(), f.id(), f.num(), f.num());
                    case "deleted" -> new Deleted(f.ix(), f.ix(), f.id(), f.num(), f.num());
                    case "consol" -> new Consolidate(f.ix(), f.ix(), f.num(), f.num());
                    case "linkrm" -> new LinkRemoved(f.ix(), f.ix(), f.id(), f.id(), f.num(),
                            f.num());
                    case "linkadd" -> new LinkAdded(f.ix(), f.ix(), f.ix(), f.id(), f.id(),
                            f.num(), f.num());
                    case "seed" -> new Seed(f.ix(), f.ix(), f.ix(), f.num(), f.num());
                    case "removed" -> new Removed(f.ix(), f.id());
                    case "unsupported", "retract" -> throw new Unsupported(path + ":" + line
                            + ": der Host machte einen Aufruf, den die Nachfahrt nicht abdeckt: "
                            + shorten(text), line);
                    default -> throw f.error("unbekannter Aufruf " + kind);
                };
                f.end();
                r.ops.add(op);
            }
        } catch (IOException ex) {
            throw new UncheckedIOException(path.toString(), ex);
        }
        return r;
    }

    private static String shorten(String s) {
        return s.length() <= 200 ? s : s.substring(0, 200) + "…";
    }

    /** Text der Zeile {@code line} (ab 1), gekürzt, für Meldungen. */
    private static String lineText(Path path, int line) {
        try (BufferedReader in = Files.newBufferedReader(path, StandardCharsets.UTF_8)) {
            String text = null;
            for (int i = 0; i < line; i++) text = in.readLine();
            return text == null ? "?" : shorten(text);
        } catch (IOException ex) {
            return "? (" + ex.getMessage() + ")";
        }
    }

    // ── Nachfahrt ──

    /** Was die Nachfahrt hält: alles, was der Host hielt. */
    private static final class World {
        final Record r;
        final List<Graph> graphs = new ArrayList<>();
        final List<Map<Id, String>> stores = new ArrayList<>();
        /** Alle Regeln in der Reihenfolge, in der der Lader sie lieferte. */
        final ArrayList<Rule> rules;
        final int ruleCapacity;
        final List<AnchorIndex> indexes = new ArrayList<>();
        final List<Engine> engines = new ArrayList<>();
        /** Engines in der Reihenfolge ihrer Konstruktion. */
        final List<Integer> order = new ArrayList<>();
        /** Graph und Speicher des letzten Aufrufs an einer Engine mit
         *  Speicher, -1 ohne. */
        int lastGraph = -1, lastStore = -1;
        long steps;
        /** Ohne Abbruch bei einer Abweichung vom Protokoll ({@link #full}):
         *  die Zeilen mit einer Abweichung werden gezählt. */
        boolean full;
        long deviations;
        int firstDeviation = -1;
        private int lastDeviation = -1;
        /** Rückgabe des letzten {@code step}: {@code T}, {@code F},
         *  {@code N}. */
        String result = "-";

        World(Record r, int ruleCapacity) {
            this.r = r;
            this.ruleCapacity = ruleCapacity;
            this.rules = new ArrayList<>(ruleCapacity);
        }

        static <T> void put(List<T> v, int ix, T value) {
            while (v.size() <= ix) v.add(null);
            v.set(ix, value);
        }

        static <T> T have(List<T> v, int ix, String what) {
            T t = ix < v.size() ? v.get(ix) : null;
            if (t == null) {
                throw new IllegalStateException("das Protokoll nutzt " + what + " " + ix
                        + ", bevor es existiert");
            }
            return t;
        }

        Graph graph(int g) { return have(graphs, g, "Graph"); }

        Map<Id, String> store(int v) { return have(stores, v, "Wertespeicher"); }

        Engine engine(int e) { return have(engines, e, "Engine"); }

        /** Die Regeln der Läufe als eigene Liste. */
        List<Rule> slice(int[] runs) {
            int n = 0;
            for (int i = 0; i < runs.length; i += 2) {
                if ((long) runs[i] + runs[i + 1] > rules.size()) {
                    throw new IllegalStateException("Regeln " + runs[i] + "+" + runs[i + 1]
                            + ", geladen sind " + rules.size());
                }
                n += runs[i + 1];
            }
            ArrayList<Rule> out = new ArrayList<>(n);
            for (int i = 0; i < runs.length; i += 2) {
                out.addAll(rules.subList(runs[i], runs[i] + runs[i + 1]));
            }
            return out;
        }

        /** Eine Abweichung an Zeile {@code no + 1}: wirft {@code m}, mit
         *  {@link #full} wird sie gezählt. */
        private void deviate(int no, Supplier<Mismatch> m) {
            if (!full) throw m.get();
            int line = no + 1;
            if (firstDeviation < 0) firstDeviation = line;
            if (line != lastDeviation) {
                deviations++;
                lastDeviation = line;
            }
        }

        Mismatch mismatch(int no, String call, String expected, String got) {
            int line = no + 1;
            return new Mismatch(r.path + ":" + line + ": " + call + ": erwartet " + expected
                    + ", bekommen " + got + "; Aufruf: " + lineText(r.path, line), line);
        }

        /** Prüft Kaskade und Graph gegen das Protokoll. */
        void check(int no, String call, int e, int g, long cascade, long nodes) {
            long c = engine(e).cascade.size();
            long n = graph(g).nodeCount();
            if (c != cascade || n != nodes) {
                deviate(no, () -> mismatch(no, call, "Kaskade " + cascade + ", Knoten " + nodes,
                        "Kaskade " + c + ", Knoten " + n));
            }
        }

        void run(int from, int to) {
            for (int no = from; no < to; no++) {
                Op op = r.ops.get(no);
                try {
                    apply(no, op);
                } catch (Mismatch | Unsupported ex) {
                    throw ex;
                } catch (RuntimeException ex) {
                    int line = no + 1;
                    throw new IllegalStateException(r.path + ":" + line + ": " + ex
                            + "; Aufruf: " + lineText(r.path, line), ex);
                }
            }
        }

        /** Ein Aufruf. Die Zweige nach Häufigkeit in den Protokollen
         *  geordnet ({@code step} zuerst). */
        private void apply(int no, Op op) {
            if (op instanceof Step o) {
                Boolean got = engine(o.e()).step(graph(o.g()), store(o.v()));
                if (!Objects.equals(got, o.result())) {
                    deviate(no, () -> mismatch(no, "step", String.valueOf(o.result()),
                            String.valueOf(got)));
                }
                result = got == null ? "N" : got ? "T" : "F";
                if (got != null) steps++;
                lastGraph = o.g();
                lastStore = o.v();
                check(no, "step", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof Connect o) {
                Id got = graph(o.g()).connect(o.a(), o.b(), o.status());
                if (!Objects.equals(got, o.id())) {
                    deviate(no, () -> mismatch(no, "connect", hex(o.id()), hex(got)));
                }
            } else if (op instanceof Base o) {
                Id got = graph(o.g()).addBaseline(o.external(), o.typ());
                if (!got.equals(o.id())) {
                    deviate(no, () -> mismatch(no, "add_baseline", hex(o.id()), hex(got)));
                }
            } else if (op instanceof Value o) {
                store(o.v()).put(o.id(), o.value());
            } else if (op instanceof Deleted o) {
                engine(o.e()).elementDeleted(graph(o.g()), o.id());
                check(no, "element_deleted", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof Added o) {
                engine(o.e()).elementsAdded(graph(o.g()), store(o.v()),
                        new ArrayList<>(Arrays.asList(o.ids())));
                lastGraph = o.g();
                lastStore = o.v();
                check(no, "elements_added", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof Consolidate o) {
                engine(o.e()).consolidate(graph(o.g()));
                check(no, "consolidate", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof AdmitDelta o) {
                engine(o.e()).admitDelta(o.domains());
            } else if (op instanceof AdmitInitial o) {
                engine(o.e()).admitInitial();
            } else if (op instanceof Changed o) {
                engine(o.e()).elementChanged(graph(o.g()), o.id());
                check(no, "element_changed", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof LinkRemoved o) {
                engine(o.e()).linkRemoved(graph(o.g()), o.a(), o.b());
                check(no, "link_removed", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof LinkAdded o) {
                engine(o.e()).linkAdded(graph(o.g()), store(o.v()), o.a(), o.b());
                lastGraph = o.g();
                lastStore = o.v();
                check(no, "link_added", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof Seed o) {
                engine(o.e()).seed(graph(o.g()), store(o.v()));
                lastGraph = o.g();
                lastStore = o.v();
                check(no, "seed", o.e(), o.g(), o.cascade(), o.nodes());
            } else if (op instanceof Removed o) {
                engine(o.e()).elementRemoved(o.id());
            } else if (op instanceof NodeStatus o) {
                Graph g = graph(o.g());
                boolean got = g.node(o.id()) != null;
                g.setNodeStatus(o.id(), o.status());
                if (got != o.done()) {
                    deviate(no, () -> mismatch(no, "set_node_status", Boolean.toString(o.done()),
                            Boolean.toString(got)));
                }
            } else if (op instanceof ConnectionStatus o) {
                Graph g = graph(o.g());
                boolean got = g.conn(o.id()) != null;
                g.setConnectionStatus(o.id(), o.status());
                if (got != o.done()) {
                    deviate(no, () -> mismatch(no, "set_connection_status",
                            Boolean.toString(o.done()), Boolean.toString(got)));
                }
            } else if (op instanceof Contradiction o) {
                engine(o.e()).sawContradiction = o.value();
            } else if (op instanceof ActiveNone o) {
                engine(o.e());
            } else if (op instanceof Mark) {
                // Grenze der Zelle edit, kein Aufruf.
            } else if (op instanceof Intern o) {
                graph(o.g()).intern(o.name());
            } else if (op instanceof GraphNew o) {
                put(graphs, o.g(), new Graph());
            } else if (op instanceof GraphClone o) {
                put(graphs, o.to(), cloneTypes(no, graph(o.from())));
            } else if (op instanceof StoreNew o) {
                put(stores, o.v(), new HashMap<>());
            } else if (op instanceof StoreClone o) {
                put(stores, o.to(), new HashMap<>(store(o.from())));
            } else if (op instanceof Load o) {
                List<Rule> loaded = Rules.load(o.json(), graph(o.g()));
                if (loaded.size() != o.count() || rules.size() != o.first()) {
                    int have = rules.size();
                    deviate(no, () -> mismatch(no, "load", o.count() + " Regeln ab Nummer "
                            + o.first(), loaded.size() + " Regeln ab Nummer " + have));
                }
                if (rules.size() + loaded.size() > ruleCapacity) {
                    int total = rules.size() + loaded.size();
                    deviate(no, () -> mismatch(no, "load", "höchstens " + ruleCapacity + " Regeln",
                            total + " Regeln"));
                }
                rules.addAll(loaded);
            } else if (op instanceof IndexBuild o) {
                put(indexes, o.i(), AnchorIndex.build(slice(o.runs())));
            } else if (op instanceof EngineNew o) {
                List<Rule> rs = slice(o.runs());
                Engine engine;
                if (o.index() < 0) {
                    engine = new Engine(rs);
                } else if (o.index() == 0) {
                    engine = new Engine(rs, (AnchorIndex) null);
                } else {
                    engine = new Engine(rs, have(indexes, o.index(), "Index"));
                }
                put(engines, o.e(), engine);
                order.add(o.e());
            } else {
                throw new IllegalStateException("Aufruf ohne Zweig: " + op);
            }
        }

        /** {@code G.clone()} eines Graphen, der nur Typen hält. */
        private Graph cloneTypes(int no, Graph from) {
            int conns = from.allConnections().size();
            if (from.nodeCount() != 0 || conns != 0) {
                int line = no + 1;
                throw new Unsupported(r.path + ":" + line + ": gclone eines Graphen mit "
                        + from.nodeCount() + " Knoten und " + conns
                        + " Verbindungen: die Java-API hat kein clone, und die innere"
                        + " Reihenfolge (byType) ist über die öffentliche API nicht kopierbar",
                        line);
            }
            Graph to = new Graph();
            int expected = 0;
            for (Map.Entry<String, Integer> t : from.typesByName.entrySet()) {
                int got = to.intern(t.getKey());
                if (got != t.getValue() || got != expected) {
                    deviate(no, () -> mismatch(no, "clone", "Typ " + t.getKey() + " mit Nummer "
                            + t.getValue(), "Nummer " + got));
                }
                expected++;
            }
            return to;
        }
    }

    private static String hex(Id id) {
        if (id == null) return "-";
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) {
            sb.append(Character.forDigit((b >> 4) & 0xF, 16))
                    .append(Character.forDigit(b & 0xF, 16));
        }
        return sb.toString();
    }

    /** Fährt das Protokoll {@code trace} nach und misst die Wellen
     *  ({@link Part#OPEN}). */
    public static Langpack.Outcome run(Path trace, Section measured) {
        return run(trace, Part.OPEN, measured);
    }

    /**
     * Fährt das Protokoll {@code trace} nach und misst {@code part} davon.
     * Das Lesen des Protokolls und der Fingerprint liegen außerhalb der
     * Messstrecke. Ergebnis: Schritte ({@code step} mit Rückgabe ungleich
     * {@code null}), angewandte Schritte (Summe der Kaskaden aller
     * Engines), Knoten und Fingerprint des Graphen der Wellen.
     *
     * @throws Mismatch bei der ersten Abweichung vom Protokoll
     * @throws Unsupported bei einem Aufruf, den die Nachfahrt nicht
     *         abbilden kann
     */
    public static Langpack.Outcome run(Path trace, Part part, Section measured) {
        Record r = read(trace);
        int[] b = r.bounds();
        int load = b[0], mark = b[1], end = r.ops.size();
        boolean anyWave = false;
        int rules = 0;
        for (Op op : r.ops) {
            if (isWave(op)) anyWave = true;
            if (op instanceof Load l) rules += l.count();
        }
        if (!anyWave) {
            throw new IllegalStateException(trace
                    + ": das Protokoll hat keinen Aufruf an einer Engine");
        }
        if (part == Part.EDIT && mark >= end) {
            throw new IllegalStateException(trace + ": das Protokoll hat keine Marke");
        }
        World w = new World(r, rules);
        Pieces pieces = new Pieces(w, measured);
        pieces.run(0, load, part == Part.LOAD);
        // Öffnen: Aufrufe des Hosts an Graph und Speicher und Aufrufe an
        // den Engines wechseln sich ab, jede Zelle zählt ihre Art.
        int at = load;
        while (at < mark) {
            boolean host = isHost(r.ops.get(at));
            int to = at + 1;
            while (to < mark && isHost(r.ops.get(to)) == host) to++;
            pieces.run(at, to, part == (host ? Part.GRAPH : Part.OPEN));
            at = to;
        }
        pieces.run(mark, end, part == Part.EDIT);
        pieces.finish();
        return outcome(trace, w, measured.wallNanos());
    }

    /** Schritte, angewandte Schritte, Knoten und Fingerprint nach der
     *  Nachfahrt, dazu die Zähler der Suchen. */
    private static Langpack.Outcome outcome(Path trace, World w, long nanos) {
        if (w.lastGraph < 0) {
            throw new IllegalStateException(trace + ": kein Aufruf an einer Engine mit Graph");
        }
        Graph g = w.graph(w.lastGraph);
        Map<Id, String> v = w.store(w.lastStore);
        List<Engine> engines = new ArrayList<>();
        for (int e : w.order) {
            Engine engine = e < w.engines.size() ? w.engines.get(e) : null;
            if (engine != null) engines.add(engine);
        }
        long applied = 0;
        long searches = 0, seedsByIndex = 0;
        for (Engine e : engines) {
            applied += e.cascade.size();
            searches = sum(searches, Counters.of(e, "searches"));
            seedsByIndex = sum(seedsByIndex, Counters.of(e, "seedsByIndex"));
        }
        return new Langpack.Outcome(w.steps, applied, g.nodeCount(),
                Langpack.fingerprint(g, engines, v), nanos, 0, searches, seedsByIndex);
    }

    /**
     * Fährt das ganze Protokoll {@code trace} nach, ohne bei einer
     * Abweichung vom Protokoll abzubrechen, und schreibt nach {@code out}
     * je Aufruf an einer Engine eine Zeile: Zeile des Protokolls, Befehl,
     * Engine ({@code e=}), Rückgabe ({@code r=}, {@code T}, {@code F},
     * {@code N} für {@code step}, sonst {@code -}), Länge der Kaskade der
     * Engine ({@code c=}) und Zahl der Knoten des Graphen ({@code n=},
     * {@code -} für einen Aufruf ohne Graph), beides nach dem Aufruf. Nach
     * {@code consol} dazu der Fingerprint des Graphen und der Kaskade
     * dieser Engine ({@code fp=}). Am Ende die Zeile {@code ende}: die Zahl
     * der Aufrufe an Engines je Befehl, die Zahl der Zeilen, deren Ergebnis
     * vom Protokoll abweicht, und die erste davon ({@code 0}: keine), dann
     * Schritte, angewandte Schritte, Knoten und Fingerprint wie
     * {@link #run}. Wirft ein Aufruf, steht die Ausnahme in einer Zeile
     * {@code abbruch}, und die Nachfahrt endet dort.
     *
     * <p>Die Aufrufe des Hosts nach der ersten Abweichung nennen Ids aus
     * den Ergebnissen von Rust und laufen unverändert, auch wo der Graph
     * die Id nicht hält. Die Ausgabe ist eine Funktion der Hauptklassen:
     * zwei Stände, die sich auf diesem Protokoll gleich verhalten, geben
     * dieselben Zeilen aus.
     *
     * @return true, wenn die Nachfahrt bis zum Ende kam
     */
    public static boolean full(Path trace, PrintStream out) {
        Record r = read(trace);
        int rules = 0;
        for (Op op : r.ops) {
            if (op instanceof Load l) rules += l.count();
        }
        World w = new World(r, rules);
        w.full = true;
        TreeMap<String, Integer> calls = new TreeMap<>();
        for (int no = 0; no < r.ops.size(); no++) {
            Op op = r.ops.get(no);
            boolean wave = isWave(op);
            String name = wave ? callName(op) : "host";
            w.result = "-";
            try {
                w.apply(no, op);
            } catch (RuntimeException ex) {
                out.println((no + 1) + " " + name + " abbruch " + ex);
                out.println("abbruch zeile=" + (no + 1) + " aufrufe=" + counts(calls)
                        + " abweichend=" + w.deviations + " erste_abweichung="
                        + Math.max(0, w.firstDeviation));
                return false;
            }
            if (!wave) continue;
            calls.merge(name, 1, Integer::sum);
            int e = engineOf(op);
            int g = graphOf(op);
            StringBuilder line = new StringBuilder();
            line.append(no + 1).append(' ').append(name).append(" e=").append(e).append(" r=")
                    .append(w.result).append(" c=").append(w.engine(e).cascade.size())
                    .append(" n=").append(g < 0 ? "-" : Long.toString(w.graph(g).nodeCount()));
            if (op instanceof Consolidate) {
                Map<Id, String> v = w.lastStore < 0 ? Map.of() : w.store(w.lastStore);
                line.append(" fp=").append(Langpack.fingerprint(w.graph(g), List.of(w.engine(e)), v));
            }
            out.println(line);
        }
        Langpack.Outcome o = outcome(r.path, w, 0);
        out.println("ende aufrufe=" + counts(calls) + " abweichend=" + w.deviations
                + " erste_abweichung=" + Math.max(0, w.firstDeviation) + " schritte=" + o.steps()
                + " angewandt=" + o.applied() + " knoten=" + o.nodes() + " fingerprint="
                + o.fingerprint());
        return true;
    }

    /** {@code befehl=zahl}, durch Kommas getrennt, nach Befehl sortiert. */
    private static String counts(TreeMap<String, Integer> calls) {
        StringBuilder sb = new StringBuilder();
        for (Map.Entry<String, Integer> c : calls.entrySet()) {
            if (sb.length() > 0) sb.append(',');
            sb.append(c.getKey()).append('=').append(c.getValue());
        }
        return sb.toString();
    }

    /** Der Befehl eines Aufrufs an einer Engine, wie im Format. */
    private static String callName(Op op) {
        if (op instanceof Step) return "step";
        if (op instanceof Added) return "added";
        if (op instanceof Deleted) return "deleted";
        if (op instanceof Changed) return "changed";
        if (op instanceof Consolidate) return "consol";
        if (op instanceof LinkRemoved) return "linkrm";
        if (op instanceof LinkAdded) return "linkadd";
        if (op instanceof Seed) return "seed";
        if (op instanceof Removed) return "removed";
        if (op instanceof AdmitDelta) return "admitd";
        if (op instanceof AdmitInitial) return "admiti";
        if (op instanceof Contradiction) return "contra";
        if (op instanceof ActiveNone) return "active";
        throw new IllegalStateException("kein Aufruf an einer Engine: " + op);
    }

    /** Die Engine eines Aufrufs an einer Engine. */
    private static int engineOf(Op op) {
        if (op instanceof Step o) return o.e();
        if (op instanceof Added o) return o.e();
        if (op instanceof Deleted o) return o.e();
        if (op instanceof Changed o) return o.e();
        if (op instanceof Consolidate o) return o.e();
        if (op instanceof LinkRemoved o) return o.e();
        if (op instanceof LinkAdded o) return o.e();
        if (op instanceof Seed o) return o.e();
        if (op instanceof Removed o) return o.e();
        if (op instanceof AdmitDelta o) return o.e();
        if (op instanceof AdmitInitial o) return o.e();
        if (op instanceof Contradiction o) return o.e();
        if (op instanceof ActiveNone o) return o.e();
        throw new IllegalStateException("kein Aufruf an einer Engine: " + op);
    }

    /** Der Graph eines Aufrufs an einer Engine, -1 für einen Aufruf ohne
     *  Graph. */
    private static int graphOf(Op op) {
        if (op instanceof Step o) return o.g();
        if (op instanceof Added o) return o.g();
        if (op instanceof Deleted o) return o.g();
        if (op instanceof Changed o) return o.g();
        if (op instanceof Consolidate o) return o.g();
        if (op instanceof LinkRemoved o) return o.g();
        if (op instanceof LinkAdded o) return o.g();
        if (op instanceof Seed o) return o.g();
        return -1;
    }

    /**
     * {@code PdeReplay voll <protokoll>}: {@link #full} nach stdout.
     * Exit-Code 0, wenn die Nachfahrt bis zum Ende kam, 1 nach einem
     * Abbruch, 2 bei falschem Aufruf.
     */
    public static void main(String[] args) {
        if (args.length != 2 || !args[0].equals("voll")) {
            System.err.println("usage: PdeReplay voll <protokoll.trace>");
            System.exit(2);
        }
        PrintStream out = new PrintStream(new BufferedOutputStream(
                new FileOutputStream(FileDescriptor.out), 1 << 16), false, StandardCharsets.UTF_8);
        boolean done = full(Path.of(args[1]), out);
        out.flush();
        System.exit(done ? 0 : 1);
    }

    /** Summe zweier Zählerstände, -1, wo einer fehlt. */
    private static long sum(long a, long b) {
        return a < 0 || b < 0 ? -1 : a + b;
    }

    /** Die Stücke der Nachfahrt, gemessen oder nicht. */
    private static final class Pieces {
        final World w;
        final Section s;
        boolean begun;

        Pieces(World w, Section s) {
            this.w = w;
            this.s = s;
        }

        void run(int from, int to, boolean measured) {
            if (!measured) {
                w.run(from, to);
                return;
            }
            if (begun) {
                s.resume();
            } else {
                s.start();
                begun = true;
            }
            w.run(from, to);
            s.pause();
        }

        void finish() {
            if (!begun) s.start();
            s.stop();
        }
    }
}
