package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicReferenceArray;
import java.util.function.IntPredicate;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.hash.Blake3;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Predicate;

/**
 * Wahl der Ankerplätze für die delta-lokale Expansion (Spec §1.6 add),
 * Spiegel von {@code engine/anchor.rs}.
 *
 * <p>Ungefiltert verankert {@link Engine#expandAt} jeden neuen Knoten
 * an jeder aktiven Regel an jeder Musterposition desselben Typs und
 * fährt je Tripel eine verankerte Suche. In einem Regelsatz, dessen
 * Strukturknoten einen Typ teilen und dessen Knoten-Korrespondenzen
 * einen Corr-Typ teilen, hat fast jede Regel eine Position vom Typ
 * eines neuen Knotens. Was die Regeln unterscheidet (der Wert eines
 * Blatts), liegt mehrere Links entfernt und wurde erst in der Suche
 * geprüft.
 *
 * <p>Der Index liest je Regel und Position die Pfade des Musters ab
 * dieser Position: einen Spannbaum der Musterlinks mit der Position
 * als Wurzel, jede Baumkante als (Richtung, Typ, Wertprädikat) des
 * nächsten Knotens. Die Pfade aller Plätze eines Typs liegen in einem
 * Präfixbaum. {@link #candidates} geht vom neuen Knoten aus einmal den
 * Graphen entlang dieses Präfixbaums und markiert jeden Pfad, den der
 * Gang verwirklicht. Ein Platz ist Kandidat, wenn alle seine Pfade
 * markiert sind und sein eigenes Wertprädikat zutrifft.
 *
 * <p>Jeder Pfad eines Matches wird vom Match selbst verwirklicht, die
 * Bedingung ist also NOTWENDIG für einen Match mit dem Knoten an der
 * Position. Sie wird auf demselben Graphzustand geprüft wie die Suche.
 * Der Gang liest Typen, Richtungen und Werte, nicht den Status der
 * Elemente auf einem Pfad: das bleibt der Suche. Die Tripel kommen in
 * der Reihenfolge der ungefilterten Schleife (Regel, Knoten, Position)
 * zurück, damit bleiben die eingetragenen Matches und ihre Reihenfolge
 * dieselben.
 *
 * <p>Nach dem Bau wird der Index nur gelesen. Mehrere Engines, auch
 * auf mehreren Threads, können einen Index teilen
 * ({@link Engine#Engine(List, AnchorIndex)}). Das Arbeitsgedächtnis
 * eines Gangs ({@link Walk}) gehört der Engine.
 */
public final class AnchorIndex {

    /** Längster Pfad ab einer Position (Links). Ein abgeschnittener
     *  Pfad ist weiter eine notwendige Bedingung. */
    static final int MAX_DEPTH = 6;
    /** Grenzen der Kodierung eines Tripels in einem {@code long}. */
    static final int MAX_RULES = 1 << 20;
    static final int MAX_POSITIONS = 1 << 12;
    /** Bis zu dieser Größe wird die Menge besuchter Paare von einem
     *  Gang zum nächsten behalten. */
    private static final int VISITED_KEEP = 1024;

    /** Richtung einer Baumkante, gesehen vom aktuellen Knoten. */
    private static final byte ANY = 0;
    /** Der aktuelle Knoten ist die Quelle der Kante. */
    private static final byte OUT = 1;
    /** Der aktuelle Knoten ist das Ziel der Kante. */
    private static final byte IN = 2;

    private static final Id ZERO = new Id(new byte[32]);
    private static final Group[] NO_GROUPS = new Group[0];
    private static final int[] NO_INTS = new int[0];

    /** Wachsende Liste von {@code int}. */
    private static final class Ints {
        int[] a = NO_INTS;
        int n;

        void add(int v) {
            if (n == a.length) a = Arrays.copyOf(a, Math.max(4, n * 2));
            a[n++] = v;
        }
    }

    /** Kind eines Präfixbaum-Knotens mit einem Prädikat, das nicht
     *  {@code equals} ist. */
    private static final class Other {
        final String key;
        final Predicate pred;
        final int child;

        Other(String key, Predicate pred, int child) {
            this.key = key; this.pred = pred; this.child = child;
        }
    }

    /**
     * Kinder eines Präfixbaum-Knotens mit gleicher Richtung und gleichem
     * Typ: ein Bereich der Beteiligungsliste bedient sie alle.
     */
    private static final class Group {
        final byte dir;
        final int typ;
        /** Grenzen des Bereichs {@code otherTyp == typ} in der
         *  Beteiligungsliste. */
        final Part lo, hi;
        /** Das Kind ohne Wertprädikat, -1 = keins. */
        int any = -1;
        /** Kinder mit {@code equals}, nach Wert. */
        HashMap<String, Integer> equals;
        /** Kinder mit einem anderen Prädikat. */
        ArrayList<Other> other;

        Group(byte dir, int typ) {
            this.dir = dir; this.typ = typ;
            lo = new Part(typ, ZERO, false, ZERO);
            hi = new Part(typ + 1, ZERO, false, ZERO);
        }
    }

    /** Eine Musterposition einer Regel als Ankerkandidat. */
    private static final class Place {
        final int rule, pos;
        /**
         * Wertprädikat der Position selbst, außer es ist {@code equals}:
         * ein solcher Platz ist unter seinem Wert abgelegt
         * ({@link Filed#equals}) und wird nur von einem Knoten erreicht,
         * der den Wert trägt.
         */
        final Predicate value;
        /** Die Präfixbaum-Knoten, die die Pfade ab der Position
         *  beenden, als Bereich von {@link AnchorIndex#needs}. */
        final int needsFrom, needsTo;

        Place(int rule, int pos, Predicate value, int needsFrom, int needsTo) {
            this.rule = rule; this.pos = pos; this.value = value;
            this.needsFrom = needsFrom; this.needsTo = needsTo;
        }
    }

    /**
     * Die an einer Stelle des Index abgelegten Plätze, getrennt nach
     * ihrem eigenen Wertprädikat: ein neuer Knoten erreicht die Plätze,
     * die nach seinem Wert fragen, über {@code equals}, ohne die
     * anzusehen, die nach einem anderen fragen.
     */
    private static final class Filed {
        /** Plätze ohne Wertprädikat oder mit einem, das nicht
         *  {@code equals} ist. */
        final Ints any = new Ints();
        /** Hält {@code any} einen Platz mit Wertprädikat? */
        boolean valued;
        /** Plätze mit {@code equals}, nach Wert. */
        HashMap<String, Ints> equals;

        int load(Predicate value) {
            if (value instanceof Predicate.Equals e) {
                Ints l = equals == null ? null : equals.get(e.expected());
                return l == null ? 0 : l.n;
            }
            return any.n;
        }

        void push(Predicate value, int place) {
            if (value instanceof Predicate.Equals e) {
                if (equals == null) equals = new HashMap<>();
                equals.computeIfAbsent(e.expected(), k -> new Ints()).add(place);
            } else {
                valued |= value != null;
                any.add(place);
            }
        }

        /** Fragt ein hier abgelegter Platz nach dem Wert des Knotens? */
        boolean asksValue() {
            return valued || equals != null;
        }
    }

    /** Plätze und Präfixbaum eines Positionstyps. */
    private static final class TypeIndex {
        /** Aufsteigend nach (Regel, Position). */
        final ArrayList<Place> places = new ArrayList<>();
        /** Wurzel des Präfixbaums. */
        final int root;
        /** Plätze ohne Pfad (Muster aus einem Knoten, oder eine
         *  Position, die kein Link verlässt): Kandidaten für jeden
         *  Knoten des Typs. */
        final Filed free = new Filed();

        TypeIndex(int root) { this.root = root; }
    }

    /** Ein (Graphknoten, Präfixbaum-Knoten)-Paar eines Gangs. */
    private record Visit(Id x, int at) {}

    /**
     * Arbeitsgedächtnis der Gänge. Gehört der Engine und überlebt die
     * Expansionen, damit eine Expansion weniger Knoten nichts dafür
     * anlegt.
     */
    static final class Walk {
        /** Je Präfixbaum-Knoten: vom aktuellen Gang verwirklicht. */
        boolean[] marked = new boolean[0];
        /** Die vom aktuellen Gang markierten Knoten (für das Zurücksetzen). */
        int[] list = new int[16];
        int listLen;
        /** Schon ausgebreitete Paare. */
        HashSet<Visit> visited = new HashSet<>();
        /** Die Tripel der laufenden Expansion. */
        long[] out = new long[64];
        int outLen;

        void mark(int id) {
            marked[id] = true;
            if (listLen == list.length) list = Arrays.copyOf(list, listLen * 2);
            list[listLen++] = id;
        }

        void push(long triple) {
            if (outLen == out.length) out = Arrays.copyOf(out, outLen * 2);
            out[outLen++] = triple;
        }
    }

    /**
     * Arbeitsgedächtnis von {@link #build}: der Breitensuche-Spannbaum
     * der Links eines Musters ab einer Position (ohne Wert-Joins, ohne
     * Selbstlinks), abgeschnitten bei {@link #MAX_DEPTH}. Ein Pfad der
     * Position ist der Weg von ihr zu einem Blatt des Baums; ein
     * Blattpfad deckt seine Präfixe.
     */
    private static final class Tree {
        boolean[] seen = new boolean[0];
        int[] parent = new int[0];
        byte[] dir = new byte[0];
        int[] depth = new int[0];
        boolean[] hasChild = new boolean[0];
        int[] queue = new int[0];

        void span(Rule r, int pos) {
            int n = r.patNodes.size();
            if (seen.length < n) {
                seen = new boolean[n];
                parent = new int[n];
                dir = new byte[n];
                depth = new int[n];
                hasChild = new boolean[n];
                queue = new int[n];
            } else {
                Arrays.fill(seen, 0, n, false);
                Arrays.fill(depth, 0, n, 0);
                Arrays.fill(hasChild, 0, n, false);
            }
            seen[pos] = true;
            int head = 0, tail = 0;
            queue[tail++] = pos;
            while (head < tail) {
                int u = queue[head++];
                if (depth[u] >= MAX_DEPTH) continue;
                for (PatLink l : r.patLinks) {
                    if (l.kind == PatLink.Kind.SAME_VALUE || l.from == l.to) continue;
                    int w;
                    boolean outgoing;
                    if (l.from == u) {
                        w = l.to;
                        outgoing = true;
                    } else if (l.to == u) {
                        w = l.from;
                        outgoing = false;
                    } else {
                        continue;
                    }
                    if (w < 0 || w >= n || seen[w]) continue;
                    seen[w] = true;
                    hasChild[u] = true;
                    parent[w] = u;
                    dir[w] = l.kind == PatLink.Kind.CONTEXT ? ANY : (outgoing ? OUT : IN);
                    depth[w] = depth[u] + 1;
                    queue[tail++] = w;
                }
            }
        }

        /** Ist {@code i} das Ende eines Pfads ab {@code pos}? */
        boolean isLeaf(int pos, int i) {
            return i != pos && seen[i] && !hasChild[i];
        }
    }

    /** Je Präfixbaum-Knoten seine Gruppen. */
    private Group[][] groups = new Group[16][];
    private int nodeCount;
    /**
     * Je Präfixbaum-Knoten die unter ihm abgelegten Plätze (Indizes in
     * {@code places} des Typs). Jeder Platz mit einem Pfad liegt unter
     * genau einem seiner {@code needs}: ein Gang erreicht die Plätze,
     * die er erfüllen kann, über die Knoten, die er markiert hat, ohne
     * die anderen Plätze des Typs anzusehen.
     */
    private final ArrayList<Filed> filed = new ArrayList<>();
    /** Die {@code needs} aller Plätze, ein Bereich je Platz. */
    private int[] needs = new int[64];
    private int needsLen;
    private final HashMap<Integer, TypeIndex> byType = new HashMap<>();
    /**
     * Je Regel und Position der Plan der dort verankerten Suche, gebaut
     * bei der ersten Suche an dem Platz. Die Pläne der Regel {@code r}
     * beginnen bei {@code planAt[r]}.
     */
    private final AtomicReferenceArray<List<Matcher.Step>> plans;
    private final int[] planAt;
    /** Digest dessen, was der Index aus den Regeln gelesen hat. */
    private final byte[] shape;
    /** Zahl der Regeln, aus denen der Index gebaut ist. */
    final int ruleCount;

    private AnchorIndex(List<Rule> rules) {
        ruleCount = rules.size();
        planAt = new int[ruleCount];
        int positions = 0;
        for (int ri = 0; ri < ruleCount; ri++) {
            planAt[ri] = positions;
            positions += rules.get(ri).patNodes.size();
        }
        plans = new AtomicReferenceArray<>(positions);
        shape = shape(rules);
    }

    /**
     * Passt die Kodierung der Tripel auf diese Regeln? Jenseits der
     * Grenzen sucht die Engine ohne Index.
     */
    public static boolean supports(List<Rule> rules) {
        if (rules.size() > MAX_RULES) return false;
        for (Rule r : rules) {
            if (r.patNodes.size() > MAX_POSITIONS) return false;
        }
        return true;
    }

    /** Liest die Pfade ab jeder Position der gerichteten Regeln. */
    public static AnchorIndex build(List<Rule> rules) {
        if (!supports(rules)) {
            throw new IllegalArgumentException("zu viele Regeln oder Musterpositionen für den Index");
        }
        AnchorIndex index = new AnchorIndex(rules);
        Tree tree = new Tree();
        int[] chain = new int[MAX_DEPTH + 1];
        for (int ri = 0; ri < rules.size(); ri++) {
            Rule rule = rules.get(ri);
            int n = rule.patNodes.size();
            for (int pos = 0; pos < n; pos++) {
                PatNode pn = rule.patNodes.get(pos);
                TypeIndex ti = index.byType.get(pn.typ);
                if (ti == null) {
                    ti = new TypeIndex(index.newNode());
                    index.byType.put(pn.typ, ti);
                }
                // Ein Pfad je Blatt des Spannbaums, die Blätter in
                // aufsteigender Position.
                tree.span(rule, pos);
                int from = index.needsLen;
                for (int leaf = 0; leaf < n; leaf++) {
                    if (!tree.isLeaf(pos, leaf)) continue;
                    int len = 0;
                    for (int w = leaf; w != pos; w = tree.parent[w]) chain[len++] = w;
                    int cur = ti.root;
                    for (int i = len - 1; i >= 0; i--) {
                        int w = chain[i];
                        PatNode next = rule.patNodes.get(w);
                        cur = index.child(cur, tree.dir[w], next.typ, next.predicate);
                    }
                    boolean known = false;
                    for (int k = from; k < index.needsLen; k++) {
                        if (index.needs[k] == cur) { known = true; break; }
                    }
                    if (!known) index.addNeed(cur);
                }
                int to = index.needsLen;
                // Den Platz unter dem need ablegen, das bisher die
                // wenigsten Plätze trägt: ein Pfad, den viele Regeln
                // teilen (der Link zum Elternknoten), füllt sich, ein
                // Pfad, den wenige teilen (ein Blattwert), bleibt kurz.
                int key = -1;
                int best = Integer.MAX_VALUE;
                for (int k = from; k < to; k++) {
                    int load = index.filed.get(index.needs[k]).load(pn.predicate);
                    if (load < best) { best = load; key = index.needs[k]; }
                }
                int placeIx = ti.places.size();
                if (key >= 0) {
                    index.filed.get(key).push(pn.predicate, placeIx);
                } else {
                    ti.free.push(pn.predicate, placeIx);
                }
                Predicate own = pn.predicate instanceof Predicate.Equals ? null : pn.predicate;
                ti.places.add(new Place(ri, pos, own, from, to));
            }
        }
        return index;
    }

    private int newNode() {
        if (nodeCount == groups.length) groups = Arrays.copyOf(groups, nodeCount * 2);
        groups[nodeCount] = NO_GROUPS;
        filed.add(new Filed());
        return nodeCount++;
    }

    private void addNeed(int node) {
        if (needsLen == needs.length) needs = Arrays.copyOf(needs, needsLen * 2);
        needs[needsLen++] = node;
    }

    /** Schlüssel eines Prädikats, das nicht {@code equals} ist: gleicher
     *  Schlüssel, gleiche Entscheidung für jeden Wert. */
    private static String key(Predicate p) {
        if (p instanceof Predicate.Exists) return "x";
        if (p instanceof Predicate.Prefix q) return "p" + q.prefix();
        if (p instanceof Predicate.Regex q) return "r" + q.raw();
        if (p instanceof Predicate.NumericRange q) {
            return "n" + Double.doubleToLongBits(q.min()) + ":" + Double.doubleToLongBits(q.max());
        }
        throw new IllegalArgumentException("unbekanntes Prädikat: " + p);
    }

    /** Das Kind von {@code parent} für einen Pfadschritt; angelegt, wenn
     *  es fehlt. */
    private int child(int parent, byte dir, int typ, Predicate pred) {
        Group group = null;
        for (Group g : groups[parent]) {
            if (g.dir == dir && g.typ == typ) { group = g; break; }
        }
        if (group == null) {
            group = new Group(dir, typ);
            Group[] old = groups[parent];
            Group[] grown = Arrays.copyOf(old, old.length + 1);
            grown[old.length] = group;
            groups[parent] = grown;
        }
        if (pred == null) {
            if (group.any < 0) group.any = newNode();
            return group.any;
        }
        if (pred instanceof Predicate.Equals e) {
            if (group.equals == null) group.equals = new HashMap<>();
            Integer c = group.equals.get(e.expected());
            if (c == null) {
                c = newNode();
                group.equals.put(e.expected(), c);
            }
            return c;
        }
        String key = key(pred);
        if (group.other == null) group.other = new ArrayList<>();
        for (Other o : group.other) {
            if (o.key.equals(key)) return o.child;
        }
        int c = newNode();
        group.other.add(new Other(key, pred, c));
        return c;
    }

    /**
     * Der Plan der Suche, die an Position {@code pos} der Regel
     * {@code rule} verankert ist. {@code r} ist diese Regel aus den
     * Regeln, für die der Index gebaut wurde.
     */
    List<Matcher.Step> plan(int rule, int pos, Rule r) {
        int i = planAt[rule] + pos;
        List<Matcher.Step> p = plans.get(i);
        if (p == null) {
            Id[] fixed = new Id[r.patNodes.size()];
            fixed[pos] = ZERO;
            plans.compareAndSet(i, null, Matcher.buildPlan(r, fixed));
            p = plans.get(i);
        }
        return p;
    }

    /**
     * Gleichen die Muster von {@code rules} denen, aus denen der Index
     * gebaut wurde? Verglichen wird ein Digest der Musterknoten (Typ,
     * Wertprädikat) und der Links aller Regeln, also alles, was der
     * Index liest. Die Typen gehen mit ihren Nummern aus der Typtabelle
     * des Graphen ein, gegen den die Regeln gesenkt wurden. Der Index
     * passt damit auch zu Regeln eines anderen Graphen, wenn dessen
     * Typtabelle die Typen gleich nummeriert, und nur dann.
     */
    public boolean fits(List<Rule> rules) {
        return rules.size() == ruleCount && Arrays.equals(shape, shape(rules));
    }

    private static void putInt(Blake3 h, byte[] buf, int v) {
        buf[0] = (byte) v;
        buf[1] = (byte) (v >> 8);
        buf[2] = (byte) (v >> 16);
        buf[3] = (byte) (v >> 24);
        h.update(buf, 0, 4);
    }

    private static void putLong(Blake3 h, byte[] buf, long v) {
        for (int i = 0; i < 8; i++) buf[i] = (byte) (v >> (8 * i));
        h.update(buf, 0, 8);
    }

    private static void putText(Blake3 h, byte[] buf, String s) {
        byte[] b = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        putInt(h, buf, b.length);
        h.update(b);
    }

    /** Digest dessen, was der Index aus einem Regelsatz liest. Jedes
     *  Feld variabler Länge trägt seine Länge voran. */
    private static byte[] shape(List<Rule> rules) {
        Blake3 h = new Blake3();
        byte[] buf = new byte[8];
        putInt(h, buf, rules.size());
        for (Rule rule : rules) {
            putInt(h, buf, rule.patNodes.size());
            for (PatNode n : rule.patNodes) {
                putInt(h, buf, n.typ);
                Predicate p = n.predicate;
                if (p == null) {
                    putInt(h, buf, 0);
                } else if (p instanceof Predicate.Exists) {
                    putInt(h, buf, 1);
                } else if (p instanceof Predicate.Equals e) {
                    putInt(h, buf, 2);
                    putText(h, buf, e.expected());
                } else if (p instanceof Predicate.Prefix q) {
                    putInt(h, buf, 3);
                    putText(h, buf, q.prefix());
                } else if (p instanceof Predicate.Regex q) {
                    putInt(h, buf, 4);
                    putText(h, buf, q.raw());
                } else if (p instanceof Predicate.NumericRange q) {
                    putInt(h, buf, 5);
                    putLong(h, buf, Double.doubleToLongBits(q.min()));
                    putLong(h, buf, Double.doubleToLongBits(q.max()));
                } else {
                    throw new IllegalArgumentException("unbekanntes Prädikat: " + p);
                }
            }
            putInt(h, buf, rule.patLinks.size());
            for (PatLink l : rule.patLinks) {
                putInt(h, buf, l.from);
                putInt(h, buf, l.to);
                putInt(h, buf, l.kind.ordinal());
            }
        }
        byte[] out = new byte[32];
        h.finalize32(out);
        return out;
    }

    // ── Tripel als long: (Regel, Index in die Knotenliste, Position) ──

    private static long pack(int rule, int ni, int pos) {
        return ((long) rule << 43) | ((long) ni << 12) | pos;
    }

    static int rule(long triple) { return (int) (triple >>> 43); }

    static int node(long triple) { return (int) ((triple >>> 12) & 0x7FFF_FFFFL); }

    static int pos(long triple) { return (int) (triple & 0xFFF); }

    /**
     * Markiert jeden Präfixbaum-Knoten unter {@code at}, den ein Gang
     * von einem Graphknoten aus verwirklicht. {@code slot} ist der Platz
     * des Knotens, an dem der Gang steht.
     */
    private void walk(Graph g, Map<Id, String> vals, Graph.Slot slot, int at, Walk st) {
        for (Group group : groups[at]) {
            boolean valued = group.equals != null || group.other != null;
            for (Part p : slot.parts.subSet(group.lo, true, group.hi, false)) {
                if (group.dir != ANY && (group.dir == OUT) != p.outgoing) continue;
                // Der Status der Kante und des Nachbarn wird hier nicht
                // gelesen. Ein Pfad über ein zurückgezogenes Element
                // wird markiert, obwohl kein Match ihn nutzen kann: die
                // Bedingung bleibt notwendig, und die Suche an dem Platz
                // scheitert an dem Element.
                if (group.any >= 0) enter(g, vals, p.other, group.any, st);
                if (valued) {
                    if (g.node(p.other) == null) continue;
                    String val = g.resolveValue(p.other, vals);
                    if (val != null && group.equals != null) {
                        Integer id = group.equals.get(val);
                        if (id != null) enter(g, vals, p.other, id, st);
                    }
                    if (group.other != null) {
                        for (Other o : group.other) {
                            if (o.pred.matches(val)) enter(g, vals, p.other, o.child, st);
                        }
                    }
                }
            }
        }
    }

    /** Der Gang hat am Graphknoten {@code x} den Präfixbaum-Knoten
     *  {@code id} erreicht: markieren und von dort weitergehen. */
    private void enter(Graph g, Map<Id, String> vals, Id x, int id, Walk st) {
        if (!st.marked[id]) st.mark(id);
        // Das Ende eines Pfads: von hier geht nichts weiter.
        if (groups[id].length == 0) return;
        // Derselbe Knoten auf demselben Pfad noch einmal (zwei Wege zu
        // ihm): sein Gang ist gelaufen. Der Gang ab der Wurzel wird
        // nicht vermerkt, er läuft einmal je Knoten und kein Pfad führt
        // zur Wurzel zurück.
        if (!st.visited.add(new Visit(x, id))) return;
        Graph.Slot s = g.map.get(x);
        if (s == null || s.node == null) return;
        walk(g, vals, s, id, st);
    }

    /**
     * Die (Regel, Index in {@code nodes}, Position)-Tripel, an denen
     * eine verankerte Suche einen Match finden kann, für die Regeln, die
     * {@code active} zulässt, aufsteigend nach (Regel, Knotenindex,
     * Position): die Reihenfolge der ungefilterten Schleife. Kodiert
     * mit {@link #rule}, {@link #node}, {@link #pos}.
     *
     * <p>{@code firstOnly}: nur die Plätze an Position 0 (für den Seed,
     * der jeden Match von seinem ersten Knoten aus sucht).
     */
    long[] candidates(Graph g, Map<Id, String> vals, List<Id> nodes, Walk st,
            IntPredicate active, boolean firstOnly) {
        if (st.marked.length != nodeCount) {
            st.marked = new boolean[nodeCount];
            st.listLen = 0;
        }
        st.outLen = 0;
        for (int ni = 0; ni < nodes.size(); ni++) {
            Id x = nodes.get(ni);
            Graph.Slot slot = g.map.get(x);
            Node node = slot == null ? null : slot.node;
            if (node == null) continue;
            TypeIndex ti = byType.get(node.typ);
            if (ti == null) continue;
            if (!node.status.matchable()) continue;
            // Leeren kostet die Kapazität, nicht die Einträge: nach
            // einem großen Gang (ein Knoten mit vielen Nachbarn) wird
            // die Menge stattdessen zurückgegeben.
            if (st.visited.size() > VISITED_KEEP) {
                st.visited = new HashSet<>();
            } else if (!st.visited.isEmpty()) {
                st.visited.clear();
            }
            walk(g, vals, slot, ti.root, st);
            // Die Plätze unter einem markierten Knoten und die Plätze
            // ohne Pfad. Jeder Platz liegt einmal, also kommt keiner
            // doppelt; sortiert wird unten. Von den Plätzen, die nach
            // einem Wert des Knotens selbst fragen, nur die, die nach
            // dem fragen, den er hat.
            //
            // Der Wert des Knotens, wo ein erreichter Platz nach ihm
            // fragt: über den Wert (equals) oder mit einem anderen
            // Prädikat (valued). Ein anderes Prädikat gegen einen nicht
            // aufgelösten Wert zu prüfen verwirft Matches.
            boolean asks = ti.free.asksValue();
            for (int i = 0; i < st.listLen && !asks; i++) {
                asks = filed.get(st.list[i]).asksValue();
            }
            String value = asks ? g.resolveValue(x, vals) : null;
            for (int i = 0; i <= st.listLen; i++) {
                Filed f = i < st.listLen ? filed.get(st.list[i]) : ti.free;
                for (int k = 0; k < f.any.n; k++) {
                    offer(ti.places.get(f.any.a[k]), ni, value, false, st, active, firstOnly);
                }
                if (value != null && f.equals != null) {
                    Ints l = f.equals.get(value);
                    if (l != null) {
                        for (int k = 0; k < l.n; k++) {
                            offer(ti.places.get(l.a[k]), ni, value, true, st, active, firstOnly);
                        }
                    }
                }
            }
            // Nur zurücksetzen, was dieser Gang markiert hat.
            for (int i = 0; i < st.listLen; i++) st.marked[st.list[i]] = false;
            st.listLen = 0;
        }
        long[] out = Arrays.copyOf(st.out, st.outLen);
        Arrays.sort(out);
        return out;
    }

    /** Ein erreichter Platz: Kandidat, wenn alle seine Pfade markiert
     *  sind und sein eigenes Wertprädikat zutrifft. */
    private void offer(Place place, int ni, String value, boolean valueHolds, Walk st,
            IntPredicate active, boolean firstOnly) {
        for (int k = place.needsFrom; k < place.needsTo; k++) {
            if (!st.marked[needs[k]]) return;
        }
        if (!valueHolds && place.value != null && !place.value.matches(value)) return;
        if (firstOnly && place.pos != 0) return;
        if (!active.test(place.rule)) return;
        st.push(pack(place.rule, ni, place.pos));
    }
}
