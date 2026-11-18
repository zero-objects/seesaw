package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.IntPredicate;
import java.util.function.Supplier;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.Rule;

/**
 * Der Seed mit seinen Suchen auf mehreren Threads (Rust
 * {@code engine::par}, Feature {@code parallel_seed}), Grundlage von
 * {@link Engine#seedWithThreads(Graph, Map, int)}.
 *
 * <p>{@link Engine#seed} sucht und traegt abwechselnd ein, aber keine
 * Suche liest, was das Eintragen schreibt: die Suchen lesen den Graphen,
 * die Regeln, die Werte und den Index, {@link Engine#record} schreibt den
 * Match-Speicher. Also koennen alle Suchen eines Seeds laufen, bevor der
 * erste Match eingetragen wird, und sie koennen nebeneinander laufen.
 * Eingetragen wird danach auf dem Thread des Aufrufers in der Reihenfolge
 * von {@code seed}: Regel fuer Regel, die Matches einer Regel in
 * mu-Ordnung.
 *
 * <p>Was eine Suche ausser dem Graphen, den Regeln, den Werten und dem
 * Index beruehrt, gehoert dem Thread: je ein {@link AnchorIndex.Walk}
 * und ein {@link Matcher.Scratch}. Die Plaene der verankerten Suchen
 * legt der Index beim ersten Gebrauch an ({@link AnchorIndex#plan}, ein
 * {@code AtomicReferenceArray}): kommen zwei Threads zugleich an einen
 * Platz, bauen beide denselben Plan, eingetragen wird einer. Gezaehlt
 * wird auf dem Thread des Aufrufers.
 */
final class Par {
    private Par() {}

    /** Knoten je Stueck der Gaenge und Kandidaten je Stueck der Suchen
     *  (Rust {@code PIECE}): klein genug, dass die Threads die Arbeit
     *  gleichmaessig teilen, gross genug, dass das Nehmen eines Stuecks
     *  nicht zaehlt. */
    static final int PIECE = 256;

    /** Arbeit am Index {@code i} mit dem Arbeitsspeicher {@code memory}
     *  des Threads, der sie faehrt. */
    interface Work<S, T> {
        T run(S memory, int i);
    }

    /** Je Thread ein Arbeitsspeicher aus {@code init}, fuer {@code n}
     *  Indizes auf bis zu {@code threads} Threads: mindestens einer. */
    static <S> List<S> memories(int n, int threads, Supplier<S> init) {
        int k = Math.max(1, Math.min(threads, n));
        List<S> out = new ArrayList<>(k);
        for (int i = 0; i < k; i++) out.add(init.get());
        return out;
    }

    /**
     * Fuehrt {@code work} fuer jeden Index unter {@code n} aus, auf so
     * vielen Threads, wie {@code memories} Arbeitsspeicher hat, jeder mit
     * seinem (Rust {@code spread}). Die Ergebnisse kommen in der Folge der
     * Indizes zurueck, gleich welcher Thread welchen Index fuhr.
     *
     * <p>Die Threads werden fuer diesen Aufruf gestartet und vor der
     * Rueckkehr beendet, auch wenn einer scheitert oder der Aufrufer
     * unterbrochen wird (die Unterbrechung bleibt danach gesetzt). Die
     * erste Ausnahme eines Threads wirft der Aufrufer, nachdem alle
     * beendet sind. Ein Arbeitsspeicher allein laeuft auf dem Thread des
     * Aufrufers.
     */
    static <S, T> List<T> spread(int n, List<S> memories, Work<S, T> work) {
        Object[] out = new Object[n];
        if (n == 0) return cast(out);
        if (memories.size() == 1) {
            S memory = memories.get(0);
            for (int i = 0; i < n; i++) out[i] = work.run(memory, i);
            return cast(out);
        }
        AtomicInteger next = new AtomicInteger();
        AtomicReference<Throwable> failed = new AtomicReference<>();
        Thread[] threads = new Thread[memories.size()];
        int started = 0;
        try {
            for (; started < threads.length; started++) {
                S memory = memories.get(started);
                Thread t = new Thread(() -> {
                    try {
                        for (int i = next.getAndIncrement(); i < n && failed.get() == null;
                                i = next.getAndIncrement()) {
                            out[i] = work.run(memory, i);
                        }
                    } catch (Throwable ex) {
                        failed.compareAndSet(null, ex);
                    }
                }, "seesaw-seed-" + started);
                threads[started] = t;
                t.start();
            }
        } catch (Throwable ex) {
            // Ein Thread liess sich nicht starten: die gestarteten
            // brechen beim naechsten Index ab.
            failed.compareAndSet(null, ex);
        } finally {
            boolean interrupted = false;
            for (int i = 0; i < started; i++) {
                while (true) {
                    try {
                        threads[i].join();
                        break;
                    } catch (InterruptedException ex) {
                        interrupted = true;
                    }
                }
            }
            if (interrupted) Thread.currentThread().interrupt();
        }
        Throwable ex = failed.get();
        if (ex instanceof RuntimeException r) throw r;
        if (ex instanceof Error e) throw e;
        if (ex != null) throw new IllegalStateException(ex);
        return cast(out);
    }

    @SuppressWarnings("unchecked")
    private static <T> List<T> cast(Object[] out) {
        return (List<T>) Arrays.asList(out);
    }

    /** Was ein verteilter Seed ueber den Index gezaehlt hat: die Gaenge
     *  ({@link AnchorIndex.Walk#visits}) und die verankerten Suchen. */
    record Counted(long visits, long searches) {}

    /** Die Matches eines Stuecks der Suchen (Rust {@code piece}): je Match
     *  seine Regel, und die Matches nacheinander. */
    private record Piece(IntList rules, ArrayList<Id[]> matches) {}

    /**
     * Der Seed ueber den {@link AnchorIndex} ({@code Engine.indexSeed})
     * mit den Gaengen und den Suchen auf bis zu {@code threads} Threads:
     * dieselben Matches in derselben Folge an {@code sink}, auf dem Thread
     * des Aufrufers (Rust {@code index_seed_spread}).
     *
     * <p>Der Gang eines Knotens und die Suche an einem Platz lesen nur den
     * Graphen, die Regeln, die Werte und den Index, sie liefern also
     * dasselbe, gleich welcher Thread sie faehrt. Die Gaenge laufen in
     * Stuecken zu {@code piece} Knoten. Die Tripel aller Stuecke, sortiert,
     * sind die Tripel eines {@link AnchorIndex#candidates} ueber alle
     * Knoten: jedes Stueck liefert seine Tripel in der Ordnung (Regel,
     * Knotenindex, Position), die Kodierung als {@code long} ordnet ebenso,
     * und kein Tripel kommt zweimal. Die Suchen laufen in Stuecken zu
     * {@code piece} Tripeln. Die Matches einer Regel aus allen Stuecken sind
     * die, die ihre Suchen nacheinander finden, {@code sink} bekommt sie
     * sortiert wie dort.
     */
    static Counted indexSeedSpread(AnchorIndex ix, List<Rule> rules, Graph g,
            Map<Id, String> vals, boolean[] admitted, int threads, int piece, Engine.Sink sink) {
        int size = Math.max(1, piece);
        IntPredicate active = ri -> admitted[ri];
        List<Id> nodes = Engine.matchableNodes(g);
        int walkPieces = (nodes.size() + size - 1) / size;
        List<AnchorIndex.Walk> walks = memories(walkPieces, threads, AnchorIndex.Walk::new);
        List<long[]> triples = spread(walkPieces, walks, (walk, p) -> {
            int from = p * size;
            int to = Math.min(from + size, nodes.size());
            long[] found = ix.candidates(g, vals, nodes.subList(from, to), walk, active,
                    (ni, id) -> id.compareTo(nodes.get(from + ni)) < 0);
            for (int k = 0; k < found.length; k++) {
                long t = found[k];
                found[k] = AnchorIndex.pack(AnchorIndex.rule(t), AnchorIndex.node(t) + from,
                        AnchorIndex.pos(t));
            }
            return found;
        });
        long visits = 0;
        for (AnchorIndex.Walk walk : walks) visits += walk.visits;
        int total = 0;
        for (long[] t : triples) total += t.length;
        long[] candidates = new long[total];
        int at = 0;
        for (long[] t : triples) {
            System.arraycopy(t, 0, candidates, at, t.length);
            at += t.length;
        }
        // (Regel, Knotenindex, Position): die Folge, die candidates ueber
        // alle Knoten zugleich liefert.
        Arrays.sort(candidates);
        int len = candidates.length;

        int searchPieces = (len + size - 1) / size;
        List<Piece> found = spread(searchPieces,
                memories(searchPieces, threads, Matcher.Scratch::new), (scratch, p) -> {
                    int from = p * size;
                    int to = Math.min(from + size, len);
                    IntList ofRule = new IntList();
                    ArrayList<Id[]> matches = new ArrayList<>();
                    for (int k = from; k < to; k++) {
                        int ri = AnchorIndex.rule(candidates[k]);
                        int pos = AnchorIndex.pos(candidates[k]);
                        Rule r = rules.get(ri);
                        Id node = nodes.get(AnchorIndex.node(candidates[k]));
                        Matcher.findMatchesPlanned(g, vals, r, ix.plan(ri, pos, r), pos, node,
                                id -> id.compareTo(node) < 0, scratch);
                        for (Id[] m : scratch.found) {
                            ofRule.add(ri);
                            matches.add(m);
                        }
                    }
                    return new Piece(ofRule, matches);
                });

        // Regel fuer Regel: die Tripel und die Matches der Regel stehen
        // in den Stuecken hintereinander.
        int next = 0;
        int piece0 = 0;
        int inPiece = 0;
        ArrayList<Id[]> ofThisRule = new ArrayList<>();
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!admitted[ri]) continue;
            Rule r = rules.get(ri);
            if (r.patNodes.isEmpty()) {
                for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, new Id[0])) sink.match(ri, m);
                continue;
            }
            while (next < len && AnchorIndex.rule(candidates[next]) == ri) next++;
            ofThisRule.clear();
            while (piece0 < found.size()) {
                Piece f = found.get(piece0);
                if (inPiece == f.rules().size()) {
                    piece0++;
                    inPiece = 0;
                    continue;
                }
                if (f.rules().get(inPiece) != ri) break;
                ofThisRule.add(f.matches().get(inPiece));
                inPiece++;
            }
            ofThisRule.sort(Matcher::compareBindings);
            for (Id[] m : ofThisRule) sink.match(ri, m);
        }
        if (next != len) {
            throw new IllegalStateException("anchor index: Kandidat einer nicht zugelassenen Regel");
        }
        return new Counted(visits, len);
    }

    /**
     * Die Matches eines Scans je Regel von {@code active}
     * ({@link Matcher#findMatchesWithFixed} ohne feste Position), die
     * Suchen auf bis zu {@code threads} Threads (Rust
     * {@code scan_spread}). Je Regel in mu-Ordnung, wie der Scan sie
     * liefert.
     */
    static List<List<Id[]>> scanSpread(List<Rule> rules, int[] active, Graph g,
            Map<Id, String> vals, int threads) {
        return spread(active.length, memories(active.length, threads, () -> null), (none, i) -> {
            Rule r = rules.get(active[i]);
            return Matcher.findMatchesWithFixed(g, vals, r, new Id[r.patNodes.size()]);
        });
    }
}
