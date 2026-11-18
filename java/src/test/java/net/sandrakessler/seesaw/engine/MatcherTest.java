package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.bench.F2p;
import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Die Suche ({@link Matcher}) gegen die Fassung von Java 2.0.2, hier
 * als {@link Alt} kopiert, in zwei Fassungen. {@code Alt.find} weicht an
 * zwei Stellen von 2.0.2 ab, beide wie Rust: {@code Alt.buildPlan} nimmt
 * Wertgleichheiten nicht in den Plan, die Abschlussprüfung
 * {@code Alt.allLinksOk} prüft sie. 2.0.2 nahm sie in den Plan und fand
 * dadurch Matches nicht
 * ({@link #wertgleichheitNurAlsAbschlussbedingungWieInRust}). Und
 * {@code Alt.sameValue} lässt zwei fehlende Werte nicht als gleich durch
 * ({@link #zweiFehlendeWerteSindNichtGleich}). {@code Alt.find202} ist
 * 2.0.2 ohne diese Abweichungen.
 *
 * <p>Verglichen wird je Stelle (Regel, feste Positionen) die Menge der
 * Matches nach {@link Matcher#compareBindings}: dieselben Bindungen wie
 * {@code Alt.find}, jede höchstens so oft und mindestens einmal. Weniger
 * oft nur dort, wo ein richtungsfreier Link der erste Link eines Schritts
 * ist und nur eine der beiden Verbindungen zwischen den Knoten matchbar
 * ist (Rust 891a63b). Für eine Regel mit Wertgleichheit dazu gegen
 * {@code Alt.find202}: jede Bindung von 2.0.2, deren Wertgleichheiten an
 * beiden Enden einen Wert haben, steht auch in der neuen Menge. Eine
 * Bindung von 2.0.2 mit zwei fehlenden Werten an einer Wertgleichheit
 * steht dort nicht. Dazu die Wirkung auf die Engine: beide Folgen von
 * Matches werden in je eine frische Engine eingetragen, deren Zustand
 * (Matches, Schlüssel, {@code byElement} in seiner Reihenfolge, To-do)
 * gleich sein muss.
 */
final class MatcherTest {

    // ── Die Suche von Java 2.0.2 (Matcher und Graph.connected). Mit
    //    treu = false bis auf die Wertgleichheit in buildPlan und sameValue
    //    unverändert, mit treu = true ganz unverändert ──

    private static final class Alt {
        static final class Step {
            int nodeIx;
            boolean preBound;
            final List<int[]> links = new ArrayList<>();
        }

        static List<Step> buildPlan(Rule r, Id[] fixed, boolean treu) {
            int n = r.patNodes.size();
            boolean[] placed = new boolean[n];
            List<Step> steps = new ArrayList<>();
            for (int i = 0; i < n; i++) {
                if (fixed[i] != null) {
                    placed[i] = true;
                    Step s = new Step();
                    s.nodeIx = i;
                    s.preBound = true;
                    steps.add(s);
                }
            }
            while (steps.size() < n) {
                int next = -1;
                for (int i = 0; i < n; i++) {
                    if (placed[i]) continue;
                    boolean connected = false;
                    for (PatLink l : r.patLinks)
                        if ((treu || l.kind != PatLink.Kind.SAME_VALUE)
                                && ((l.from == i && placed[l.to]) || (l.to == i && placed[l.from]))) {
                            connected = true;
                            break;
                        }
                    if (connected) { next = i; break; }
                    if (next < 0) next = i;
                }
                Step s = new Step();
                s.nodeIx = next;
                for (PatLink l : r.patLinks) {
                    int code;
                    if (l.kind == PatLink.Kind.SAME_VALUE) {
                        // Abweichung von 2.0.2 ohne treu: keine
                        // Wertgleichheit im Plan.
                        if (!treu) continue;
                        code = 2;
                    } else if (l.kind == PatLink.Kind.CONTEXT) {
                        code = -1;
                    } else {
                        code = -9;
                    }
                    if (l.from == next && placed[l.to]) {
                        s.links.add(new int[] {l.to, code == -9 ? 1 : code});
                    } else if (l.to == next && placed[l.from]) {
                        s.links.add(new int[] {l.from, code == -9 ? 0 : code});
                    }
                }
                placed[next] = true;
                steps.add(s);
            }
            return steps;
        }

        static boolean connected(Graph g, Id s, Id t) {
            Conn c = g.conn(Ident.identConnection(s, t));
            return c != null && c.status.matchable();
        }

        static boolean nodeOk(Graph g, Map<Id, String> vals, PatNode pn, Id id) {
            Node n = g.node(id);
            if (n == null || !n.status.matchable() || n.typ != pn.typ) return false;
            return pn.predMatches(g.resolveValue(id, vals));
        }

        static boolean linksOk(Graph g, Map<Id, String> vals, Id[] cur, List<int[]> links,
                Id cand, boolean treu) {
            for (int[] l : links) {
                Id other = cur[l[0]];
                boolean ok;
                if (l[1] == 2) {
                    ok = sameValue(g, vals, cand, other, treu);
                } else if (l[1] == 1) {
                    ok = connected(g, cand, other);
                } else if (l[1] == 0) {
                    ok = connected(g, other, cand);
                } else {
                    ok = connected(g, cand, other) || connected(g, other, cand);
                }
                if (!ok) return false;
            }
            return true;
        }

        static boolean sameValue(Graph g, Map<Id, String> vals, Id a, Id b, boolean treu) {
            String va = g.resolveValue(a, vals);
            String vb = g.resolveValue(b, vals);
            // 2.0.2: null gleicht nur null.
            if (treu) return va == null ? vb == null : va.equals(vb);
            // Abweichung von 2.0.2 ohne treu: zwei fehlende Werte sind
            // nicht gleich.
            return va != null && va.equals(vb);
        }

        static boolean allLinksOk(Graph g, Map<Id, String> vals, Rule r, Id[] cur,
                boolean treu) {
            for (PatLink l : r.patLinks) {
                Id s = cur[l.from];
                Id t = cur[l.to];
                if (s == null || t == null) return false;
                boolean ok;
                switch (l.kind) {
                    case SAME_VALUE:
                        ok = sameValue(g, vals, s, t, treu);
                        break;
                    case CONTEXT:
                        ok = connected(g, s, t) || connected(g, t, s);
                        break;
                    default:
                        ok = connected(g, s, t);
                        break;
                }
                if (!ok) return false;
            }
            return true;
        }

        static void enumerate(Graph g, Map<Id, String> vals, Rule r, List<Step> plan, int depth,
                Id[] cur, List<Id[]> out, boolean treu) {
            if (depth == plan.size()) {
                if (allLinksOk(g, vals, r, cur, treu)) out.add(cur.clone());
                return;
            }
            Step step = plan.get(depth);
            PatNode pn = r.patNodes.get(step.nodeIx);
            if (step.preBound) {
                if (nodeOk(g, vals, pn, cur[step.nodeIx]))
                    enumerate(g, vals, r, plan, depth + 1, cur, out, treu);
                return;
            }
            List<Id> candidates = new ArrayList<>();
            if (!step.links.isEmpty()) {
                int[] first = step.links.get(0);
                Id anchor = cur[first[0]];
                for (Part p : g.partsByOtherType(anchor, pn.typ)) {
                    if (first[1] == 1 && p.outgoing) continue;
                    if (first[1] == 0 && !p.outgoing) continue;
                    candidates.add(p.other);
                }
            } else {
                candidates = g.nodesOfType(pn.typ);
            }
            for (Id cand : candidates) {
                boolean dup = false;
                for (Id c : cur) if (cand.equals(c)) { dup = true; break; }
                if (dup) continue;
                if (!nodeOk(g, vals, pn, cand)) continue;
                if (!linksOk(g, vals, cur, step.links, cand, treu)) continue;
                cur[step.nodeIx] = cand;
                enumerate(g, vals, r, plan, depth + 1, cur, out, treu);
                cur[step.nodeIx] = null;
            }
        }

        static List<Id[]> find(Graph g, Map<Id, String> vals, Rule r, Id[] fixed,
                boolean treu) {
            List<Step> plan = buildPlan(r, fixed, treu);
            Id[] cur = fixed.clone();
            List<Id[]> out = new ArrayList<>();
            enumerate(g, vals, r, plan, 0, cur, out, treu);
            out.sort(Matcher::compareBindings);
            return out;
        }

        /** Mit den beiden Abweichungen wie Rust. */
        static List<Id[]> find(Graph g, Map<Id, String> vals, Rule r, Id[] fixed) {
            return find(g, vals, r, fixed, false);
        }

        /** Wie Java 2.0.2: Wertgleichheit im Plan (Code 2), zwei fehlende
         *  Werte gleich. */
        static List<Id[]> find202(Graph g, Map<Id, String> vals, Rule r, Id[] fixed) {
            return find(g, vals, r, fixed, true);
        }
    }

    // ── Vergleich ──

    private static String text(Id[] m) {
        return Engine.key(0, m).substring(1);
    }

    private static String text(List<Id[]> ms) {
        StringBuilder sb = new StringBuilder();
        for (Id[] m : ms) sb.append(text(m)).append('\n');
        return sb.toString();
    }

    /**
     * {@code alt} und {@code neu}, beide nach compareBindings sortiert:
     * dieselben Bindungen, jede in {@code neu} mindestens einmal und
     * höchstens so oft wie in {@code alt}. Rückgabe: Zahl der in
     * {@code neu} weggefallenen Wiederholungen.
     */
    private static int gleicheMenge(String wo, List<Id[]> alt, List<Id[]> neu) {
        int i = 0, j = 0, weg = 0;
        while (i < alt.size() || j < neu.size()) {
            if (i >= alt.size()) fail(wo + ": nur neu " + text(neu.get(j)));
            if (j >= neu.size()) fail(wo + ": nur alt " + text(alt.get(i)));
            Id[] m = alt.get(i);
            if (Matcher.compareBindings(m, neu.get(j)) != 0) {
                fail(wo + ": alt " + text(m) + ", neu " + text(neu.get(j)));
            }
            int a = 0, b = 0;
            while (i < alt.size() && Matcher.compareBindings(alt.get(i), m) == 0) { a++; i++; }
            while (j < neu.size() && Matcher.compareBindings(neu.get(j), m) == 0) { b++; j++; }
            assertTrue(b <= a, wo + ": " + text(m) + " neu " + b + "-mal, alt " + a + "-mal");
            weg += a - b;
        }
        return weg;
    }

    /** Steht {@code m} in {@code ms} (nach compareBindings sortiert)? */
    private static boolean enthalten(List<Id[]> ms, Id[] m) {
        return Collections.binarySearch(ms, m, Matcher::compareBindings) >= 0;
    }

    private static boolean mitWertgleichheit(Rule r) {
        for (PatLink l : r.patLinks) if (l.kind == PatLink.Kind.SAME_VALUE) return true;
        return false;
    }

    /** Zustand einer Engine, die nur eingetragen hat: Matches, Schlüssel,
     *  byElement in seiner Reihenfolge, To-do. */
    private static String zustand(Engine e) {
        StringBuilder sb = new StringBuilder();
        for (Engine.MatchRec m : e.matches) {
            sb.append("m ").append(Engine.key(m.ruleIx, m.refs)).append(' ').append(m.dead)
                    .append(' ').append(m.entry).append('\n');
        }
        TreeMap<String, Integer> keys = new TreeMap<>();
        for (Map.Entry<MatchKey, Integer> k : e.byKey.entrySet()) {
            keys.put(Engine.key(k.getKey().ruleIx, k.getKey().refs), k.getValue());
        }
        for (Map.Entry<String, Integer> k : keys.entrySet()) {
            sb.append("k ").append(k.getKey()).append(' ').append(k.getValue()).append('\n');
        }
        for (Map.Entry<Id, IntList> x : e.byElement.entrySet()) {
            sb.append("e ").append(text(new Id[] {x.getKey()})).append(' ').append(x.getValue())
                    .append('\n');
        }
        for (Engine.TodoKey t : e.todo) {
            sb.append("t ").append(t.rank).append(' ').append(Engine.key(t.ruleIx, t.refs))
                    .append('\n');
        }
        return sb.toString();
    }

    /** Vergleicht alle Stellen eines Graphen und trägt die Ergebnisse in
     *  zwei frische Engines ein. */
    private static final class Vergleich {
        final Graph g;
        final Map<Id, String> vals;
        final List<Rule> rules;
        final AnchorIndex ix;
        final Engine alt;
        final Engine neu;
        /** Arbeitsspeicher wie in der Engine, über alle Stellen behalten. */
        final Matcher.Scratch scratch = new Matcher.Scratch();
        int suchen;
        int matches;
        int weg;
        /** Stellen, deren Plan Links hat, die kein Schritt prüft
         *  ({@code Plan.closing}). */
        int mitClosing;
        /** Vergleich mit {@code Alt.find202} an den Stellen einer Regel mit
         *  Wertgleichheit: Bindungen von 2.0.2 mit Werten (stehen in der
         *  neuen Menge), Bindungen von 2.0.2 mit zwei fehlenden Werten an
         *  einer Wertgleichheit (stehen dort nicht) und Bindungen der neuen
         *  Menge, die 2.0.2 nicht fand. */
        int mitWerten202;
        int ohneWert202;
        int nurNeu202;

        Vergleich(Graph g, Map<Id, String> vals, List<Rule> rules) {
            this.g = g;
            this.vals = vals;
            this.rules = rules;
            this.ix = AnchorIndex.build(rules);
            this.alt = new Engine(rules, false);
            this.neu = new Engine(rules, false);
        }

        /** Eine Stelle: Regel {@code ri}, feste Positionen {@code fixed}. */
        void stelle(String wo, int ri, Id[] fixed) {
            Rule r = rules.get(ri);
            List<Id[]> a = Alt.find(g, vals, r, fixed);
            List<Id[]> n = Matcher.findMatchesWithFixed(g, vals, r, fixed);
            String w = wo + " Regel " + r.name + " fest " + text(fixed(fixed));
            weg += gleicheMenge(w, a, n);
            if (mitWertgleichheit(r)) wie202(w, r, fixed, n);
            // Der verankerte Weg der Engine (Plan aus dem Index) liefert
            // dieselbe Folge wie findMatchesWithFixed.
            int pos = -1, fest = 0;
            for (int i = 0; i < fixed.length; i++) if (fixed[i] != null) { pos = i; fest++; }
            if (fest == 1) {
                Matcher.findMatchesPlanned(g, vals, r, ix.plan(ri, pos, r), pos, fixed[pos],
                        scratch);
                assertEquals(text(n), text(scratch.found), w + ": verankerter Weg");
            }
            // Die alte Folge über record ohne Graph (Ids der Verbindungen
            // abgeleitet wie in 2.0.2), die neue über record mit Graph (Ids
            // aus den Participations), wie die Engine sie einträgt.
            for (Id[] m : a) alt.record(ri, m);
            for (Id[] m : n) neu.record(g, ri, m);
            if (Matcher.plan(r, fixed).closing.length > 0) mitClosing++;
            suchen++;
            matches += n.size();
        }

        /**
         * Die Matches {@code n} der Suche gegen die von Java 2.0.2
         * ({@code Alt.find202}) an derselben Stelle. Jede Bindung von 2.0.2,
         * deren Wertgleichheiten an beiden Enden einen Wert haben, steht in
         * {@code n}. Eine Bindung von 2.0.2 mit fehlendem Wert an einer
         * Wertgleichheit (2.0.2 ließ dort nur zwei fehlende Werte durch)
         * steht nicht in {@code n}.
         */
        void wie202(String w, Rule r, Id[] fixed, List<Id[]> n) {
            List<Id[]> a = Alt.find202(g, vals, r, fixed);
            for (Id[] m : a) {
                boolean drin = enthalten(n, m);
                if (alleWerte(r, m)) {
                    assertTrue(drin, w + ": 2.0.2 fand " + text(m) + ", die Suche nicht");
                    mitWerten202++;
                } else {
                    assertTrue(!drin, w + ": " + text(m) + " mit zwei fehlenden Werten");
                    ohneWert202++;
                }
            }
            for (Id[] m : n) if (!enthalten(a, m)) nurNeu202++;
        }

        /** Haben beide Enden jeder Wertgleichheit von {@code r} in
         *  {@code m} einen Wert? */
        boolean alleWerte(Rule r, Id[] m) {
            for (PatLink l : r.patLinks) {
                if (l.kind != PatLink.Kind.SAME_VALUE) continue;
                if (g.resolveValue(m[l.from], vals) == null) return false;
                if (g.resolveValue(m[l.to], vals) == null) return false;
            }
            return true;
        }

        /** Jede Regel mit je zwei festen Positionen, besetzt aus einem
         *  Match der Suche ohne feste Position (2.0.2). Der Plan nennt
         *  dann die Links zwischen den beiden und die Selbstlinks in
         *  {@code Plan.closing}. */
        void paare(String wo) {
            for (int ri = 0; ri < rules.size(); ri++) {
                Rule r = rules.get(ri);
                int n = r.patNodes.size();
                for (Id[] m : Alt.find(g, vals, r, new Id[n])) {
                    for (int i = 0; i < n; i++) {
                        for (int j = i + 1; j < n; j++) {
                            Id[] fixed = new Id[n];
                            fixed[i] = m[i];
                            fixed[j] = m[j];
                            stelle(wo, ri, fixed);
                        }
                    }
                }
            }
        }

        private static Id[] fixed(Id[] fixed) {
            Id[] out = new Id[fixed.length];
            for (int i = 0; i < fixed.length; i++) {
                out[i] = fixed[i] == null ? new Id(new byte[32]) : fixed[i];
            }
            return out;
        }

        /** Jede Regel ohne feste Position und an jeder Position mit jedem
         *  Knoten ihres Typs, gleich welchen Status. Dazu connected,
         *  connectedAlive und connectionId an jeder Participation in beiden
         *  Richtungen gegen die Ableitung. */
        void alle(String wo) {
            verbindungen(wo, g);
            TreeMap<Integer, List<Id>> byType = new TreeMap<>();
            for (Node n : g.allNodes()) byType.computeIfAbsent(n.typ, k -> new ArrayList<>()).add(n.id);
            for (List<Id> ids : byType.values()) ids.sort(Id::compareTo);
            for (int ri = 0; ri < rules.size(); ri++) {
                Rule r = rules.get(ri);
                int n = r.patNodes.size();
                stelle(wo, ri, new Id[n]);
                for (int pos = 0; pos < n; pos++) {
                    for (Id id : byType.getOrDefault(r.patNodes.get(pos).typ, List.of())) {
                        Id[] fixed = new Id[n];
                        fixed[pos] = id;
                        stelle(wo, ri, fixed);
                    }
                }
            }
        }

        void engineGleich(String wo) {
            assertEquals(zustand(alt), zustand(neu), wo + ": Wirkung auf die Engine");
            System.out.println("MatcherTest " + wo + ": suchen=" + suchen + " matches=" + matches
                    + " weggefallen=" + weg + " eingetragen=" + neu.matches.size()
                    + " wie202: mit_werten=" + mitWerten202 + " ohne_wert=" + ohneWert202
                    + " nur_neu=" + nurNeu202);
        }
    }

    /** connected, connectedAlive und connectionId für (a, b) gleich der
     *  Ableitung der Id wie in 2.0.2. */
    private static void verbindung(String wo, Graph g, Id a, Id b) {
        Id abgeleitet = Ident.identConnection(a, b);
        Conn c = g.conn(abgeleitet);
        String w = wo + " " + text(new Id[] {a, b});
        assertEquals(Alt.connected(g, a, b), g.connected(a, b), w + ": connected");
        assertEquals(c != null && c.status.alive(), g.connectedAlive(a, b), w + ": connectedAlive");
        Id id = g.connectionId(a, b);
        if (c == null) {
            assertEquals(null, id, w + ": connectionId ohne Verbindung");
        } else {
            assertEquals(abgeleitet, id, w + ": connectionId");
        }
    }

    /** {@link #verbindung} an jeder Participation jedes Knotens, in beiden
     *  Richtungen. */
    private static void verbindungen(String wo, Graph g) {
        for (Node n : g.allNodes()) {
            for (Part p : g.parts(n.id)) {
                verbindung(wo, g, n.id, p.other);
                verbindung(wo, g, p.other, n.id);
            }
        }
    }

    /**
     * {@link Graph#connectionId} liest die Id aus der Beteiligungsliste
     * der Quelle, als Array und als TreeSet (mehr als 48 Einträge): für
     * jedes geordnete Paar gleich der Ableitung, wo die Verbindung
     * besteht, sonst null. connected und connectedAlive wie in 2.0.2.
     */
    @Test
    void verbindungAusDerBeteiligungsliste() {
        Graph g = new Graph();
        Id hub = g.addBaseline("hub", "H");
        List<Id> alle = new ArrayList<>(List.of(hub));
        St[] status = {St.SOLID, St.GHOST, St.TENTATIVE_TOMBSTONE, St.TOMBSTONE};
        for (int i = 0; i < 60; i++) {
            Id x = g.addBaseline("x" + i, i % 2 == 0 ? "A" : "B");
            alle.add(x);
            Id e = i % 3 == 0 ? g.connect(x, hub, St.SOLID) : g.connect(hub, x, St.SOLID);
            g.setConnectionStatus(e, status[i % 4]);
            if (i % 5 == 0) g.connect(i % 3 == 0 ? hub : x, i % 3 == 0 ? x : hub, St.GHOST);
            if (i > 0 && i % 7 == 0) g.connect(x, alle.get(i), St.SOLID);
        }
        Id selbst = g.addBaseline("selbst", "A");
        alle.add(selbst);
        g.connect(selbst, selbst, St.SOLID);
        alle.add(Ident.identBaseline("fehlt"));
        assertTrue(g.map.get(hub).parts.size() > 48, "Liste des Hubs als TreeSet");
        for (Id a : alle) for (Id b : alle) verbindung("paar", g, a, b);
    }

    private static Rule regel(Graph g, String name, List<String> typen, List<PatLink> links) {
        Rule r = new Rule();
        r.name = name;
        r.rank = 1;
        r.patNodes = new ArrayList<>();
        for (String t : typen) r.patNodes.add(new PatNode(g.intern(t), null));
        r.patLinks = new ArrayList<>(links);
        r.createNodes = new ArrayList<>();
        r.createLinks = new ArrayList<>();
        r.inputTypes = new ArrayList<>();
        return r;
    }

    private static long saturate(Engine e, Graph g, Map<Id, String> vs) {
        long steps = 0;
        while (e.step(g, vs) != null) steps++;
        return steps;
    }

    // ── Welten ──

    /**
     * Ein richtungsfreier Link zwischen a und b, im Graphen beide
     * Verbindungen a→b und b→a. Ist nur eine matchbar, liefert die Suche
     * den Match einmal (2.0.2: zweimal, je Participation einmal). Sind
     * beide matchbar, zweimal wie bisher.
     */
    @Test
    void richtungsfreierLinkEinerVerbindungLiefertDenMatchEinmal() {
        Graph g = new Graph();
        Map<Id, String> vals = Map.of();
        Id a = g.addBaseline("a", "X");
        Id b = g.addBaseline("b", "Y");
        g.connect(a, b, St.SOLID);
        Id ba = g.connect(b, a, St.SOLID);
        Rule r = regel(g, "Kontext", List.of("X", "Y"), List.of(PatLink.context(0, 1)));
        List<Id[]> stellen = List.of(new Id[] {a, null}, new Id[] {null, b}, new Id[2]);
        Vergleich v = new Vergleich(g, vals, List.of(r));
        for (St st : List.of(St.TOMBSTONE, St.SOLID, St.TENTATIVE_TOMBSTONE)) {
            g.setConnectionStatus(ba, st);
            int erwartet = st == St.TOMBSTONE ? 1 : 2;
            for (Id[] fixed : stellen) {
                String wo = st + " fest " + text(Vergleich.fixed(fixed));
                assertEquals(2, Alt.find(g, vals, r, fixed).size(), wo + ": 2.0.2");
                List<Id[]> neu = Matcher.findMatchesWithFixed(g, vals, r, fixed);
                assertEquals(erwartet, neu.size(), wo);
                for (Id[] m : neu) assertArrayEquals(new Id[] {a, b}, m, wo);
            }
            v.alle(st.toString());
        }
        assertEquals(3, v.weg);
        v.engineGleich("richtungsfrei");
        assertEquals(1, v.neu.matches.size());
    }

    /**
     * Zwei feste Positionen aus einem bekannten Match und ein Muster mit
     * Selbstlink. Links, die kein Schritt prüft ({@code Plan.closing}),
     * hat ein Plan nur mit einem Selbstlink oder einem Link zwischen zwei
     * festen Positionen, hier je Plan beide. Die Suche liefert dieselbe
     * Folge wie die von 2.0.2, auch wenn eine dieser Verbindungen nicht
     * matchbar ist. Die Matches stammen aus dem Graphen, in dem alle
     * Verbindungen SOLID sind.
     */
    @Test
    void zweiFestePositionenUndSelbstlinkWieInJava202() {
        Graph g = new Graph();
        Map<Id, String> vals = Map.of();
        Id a = g.addBaseline("a", "X");
        Id a2 = g.addBaseline("a2", "X");
        Id b = g.addBaseline("b", "Y");
        Id b2 = g.addBaseline("b2", "Y");
        Id c = g.addBaseline("c", "Z");
        Id c2 = g.addBaseline("c2", "Z");
        g.connect(a, b, St.SOLID);
        g.connect(a, b2, St.SOLID);
        g.connect(a2, b, St.SOLID);
        Id bc = g.connect(b, c, St.SOLID);
        g.connect(b2, c, St.SOLID);
        g.connect(b, c2, St.SOLID);
        Id cc = g.connect(c, c, St.SOLID);
        g.connect(c2, c2, St.SOLID);
        Id ac = g.connect(a, c, St.SOLID);
        g.connect(c2, a2, St.SOLID);
        // X → Y → Z, Z → Z, X und Z richtungsfrei verbunden.
        Rule r = regel(g, "Selbstlink", List.of("X", "Y", "Z"),
                List.of(PatLink.directed(0, 1), PatLink.directed(1, 2), PatLink.directed(2, 2),
                        PatLink.context(0, 2)));
        List<Id[]> bekannt = Alt.find(g, vals, r, new Id[3]);
        assertEquals(3, bekannt.size(), "Matches im Graphen mit allen Verbindungen SOLID");
        int[][] paare = {{0, 1}, {0, 2}, {1, 2}};
        int geschlossen = 0;
        St[] status = {St.SOLID, St.GHOST, St.TENTATIVE_TOMBSTONE, St.TOMBSTONE};
        for (St sCc : status) {
            for (St sAc : status) {
                g.setConnectionStatus(cc, sCc);
                g.setConnectionStatus(ac, sAc);
                g.setConnectionStatus(bc, sAc == St.GHOST ? St.TOMBSTONE : St.SOLID);
                for (Id[] m : bekannt) {
                    for (int[] p : paare) {
                        Id[] fixed = new Id[3];
                        fixed[p[0]] = m[p[0]];
                        fixed[p[1]] = m[p[1]];
                        Matcher.Plan plan = Matcher.plan(r, fixed);
                        assertTrue(plan.closing.length > 0, "Selbstlink in closing");
                        String wo = "cc " + sCc + ", ac " + sAc + ", fest " + text(Vergleich.fixed(fixed));
                        List<Id[]> alt = Alt.find(g, vals, r, fixed);
                        List<Id[]> neu = Matcher.findMatchesWithFixed(g, vals, r, fixed);
                        assertEquals(text(alt), text(neu), wo);
                        geschlossen += plan.closing.length;
                    }
                }
            }
        }
        // Je Plan der Selbstlink und der Link zwischen den beiden festen
        // Positionen.
        assertEquals(16 * 3 * 3 * 2, geschlossen);
        Vergleich v = new Vergleich(g, vals, List.of(r));
        for (St st : status) {
            g.setConnectionStatus(cc, st);
            v.paare(st.toString());
        }
        v.engineGleich("selbstlink");
    }

    private static Id blatt(Graph g, Map<Id, String> vals, String name, String typ, String wert) {
        Id id = g.addBaseline(name, typ);
        vals.put(id, wert);
        return id;
    }

    /**
     * Wertgleichheit als einziger Link: die Kandidaten der zweiten
     * Position sind alle Knoten ihres Typs, die Abschlussprüfung
     * vergleicht die Werte. Jeder Knoten mit gleichem Wert kommt genau
     * einmal, gleich ob und wie er mit dem ersten verbunden ist: b über
     * beide Richtungen, d gar nicht, e über eine Verbindung im Tombstone.
     * Java 2.0.2 nahm die Kandidaten aus den Participations des gesetzten
     * Knotens, dort fehlte d und b kam zweimal.
     */
    @Test
    void wertgleichheitOhneVerbindungZwischenDenEnden() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id a = blatt(g, vals, "a", "L1", "v");
        Id b = blatt(g, vals, "b", "L2", "v");
        Id c = blatt(g, vals, "c", "L2", "w");
        Id d = blatt(g, vals, "d", "L2", "v");
        Id e = blatt(g, vals, "e", "L2", "v");
        g.connect(a, b, St.SOLID);
        g.connect(b, a, St.SOLID);
        g.connect(a, c, St.SOLID);
        Id ae = g.connect(a, e, St.SOLID);
        g.setConnectionStatus(ae, St.TOMBSTONE);
        Rule r = regel(g, "Wert", List.of("L1", "L2"), List.of(PatLink.sameValue(0, 1)));
        List<Id[]> ab = List.of(new Id[] {a, b}, new Id[] {a, d}, new Id[] {a, e});
        ArrayList<Id[]> erwartet = new ArrayList<>(ab);
        erwartet.sort(Matcher::compareBindings);
        for (Id[] fixed : List.of(new Id[] {a, null}, new Id[2])) {
            assertEquals(text(erwartet), text(Alt.find(g, vals, r, fixed)), "Alt");
            assertEquals(text(erwartet), text(Matcher.findMatchesWithFixed(g, vals, r, fixed)));
        }
        Id[][][] stellen = {
            {{null, b}, {a, b}},
            {{null, d}, {a, d}},
            {{a, d}, {a, d}},
            {{a, c}},
        };
        for (Id[][] s : stellen) {
            List<Id[]> soll = new ArrayList<>();
            for (int i = 1; i < s.length; i++) soll.add(s[i]);
            assertEquals(text(soll), text(Alt.find(g, vals, r, s[0])), "Alt");
            assertEquals(text(soll), text(Matcher.findMatchesWithFixed(g, vals, r, s[0])));
        }
        Vergleich v = new Vergleich(g, vals, List.of(r));
        v.alle("wert");
        assertEquals(0, v.weg);
        v.engineGleich("wert");
        // 2.0.2 fand (a, b) und (a, e), alle Werte vorhanden, nicht (a, d).
        assertTrue(v.mitWerten202 > 0, "Bindungen von 2.0.2 mit Werten");
        assertEquals(0, v.ohneWert202);
        assertTrue(v.nurNeu202 > 0, "(a, d) fehlt in 2.0.2");
    }

    /**
     * Die Regel {@code TestUnit_JUnit5_Plan} der Engine {@code test-unit}
     * aus den Protokollen von zerodds-pde ({@code open2} Zeile 396),
     * verkürzt auf die Knoten, an denen der Fehler liegt: links
     * R → p → pn, rechts P → cu → xn mit cu und xn als Kontext, die
     * references-Corr R ↔ P, die establishes-Corr p ↔ cu und der Join
     * pn = xn. Im Original liegen zwischen cu und xn weitere
     * Kontextknoten.
     */
    private static final String JUNIT5_PLAN = "{\"format\":3,\"name\":\"test-unit\",\"rules\":["
            + "{\"name\":\"TestUnit_JUnit5_Plan\",\"rank\":865,"
            + "\"left\":{\"anchor\":\"p\",\"nodes\":["
            + "{\"name\":\"R\",\"type\":\"tst:Root\"},"
            + "{\"name\":\"p\",\"type\":\"tst:Node\"},"
            + "{\"name\":\"pn\",\"type\":\"name\"}],"
            + "\"links\":[[\"R\",\"p\"],[\"p\",\"pn\"]]},"
            + "\"right\":{\"anchor\":\"cu\",\"nodes\":["
            + "{\"name\":\"P\",\"type\":\"sl:Project\"},"
            + "{\"name\":\"cu\",\"type\":\"sl:Node\",\"context\":true},"
            + "{\"name\":\"xn\",\"type\":\"name\",\"context\":true}],"
            + "\"links\":[[\"P\",\"cu\"],[\"cu\",\"xn\"]]},"
            + "\"corrs\":["
            + "{\"type\":\"Ctestunit_Root\",\"left\":\"R\",\"right\":\"P\",\"role\":\"references\","
            + "\"bindings\":[]},"
            + "{\"type\":\"Ctestunit_Node\",\"left\":\"p\",\"right\":\"cu\",\"role\":\"establishes\","
            + "\"bindings\":[{\"left\":\"pn\",\"right\":\"xn\"}]}],"
            + "\"joins\":[[\"pn\",\"xn\"]]}]}";

    /**
     * Wertgleichheit nur als abschließende Bedingung, wie Rust
     * ({@code build_plan} in {@code src/engine/matcher.rs}). Vorwärts
     * stehen im Muster R, p, pn, cu, xn, die Corr und P an den Positionen
     * 0 bis 6. Java 2.0.2 setzt nach pn als nächste Position xn, weil die
     * Wertgleichheit pn = xn als Verbindung zählt, und sucht die
     * Kandidaten für xn unter den Nachbarn von pn. Dort steht kein Knoten
     * des Typs name, also kein Match. Rust erreicht xn über die Corr, P
     * und cu und prüft pn = xn am Ende. Erwartet wird der eine Match ohne
     * feste Position, an jeder festen Position und auf dem verankerten Weg
     * der Engine. Mit verschiedenen Werten an pn und xn keiner.
     */
    @Test
    void wertgleichheitNurAlsAbschlussbedingungWieInRust() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id[] match = junit5Graph(g, vals, "Plan", "Plan");
        Id xn = match[4];
        Rule fwd = Rules.load(JUNIT5_PLAN, g).get(0);
        assertEquals("TestUnit_JUnit5_Plan→", fwd.name);
        String[] typen = {"tst:Root", "tst:Node", "name", "sl:Node", "name", "Ctestunit_Root",
            "sl:Project"};
        assertEquals(typen.length, fwd.patNodes.size(), "Positionen im Muster");
        for (int i = 0; i < typen.length; i++) {
            assertEquals(g.lookup(typen[i]), fwd.patNodes.get(i).typ, "Typ an Position " + i);
        }
        int n = match.length;
        AnchorIndex ix = AnchorIndex.build(List.of(fwd));
        Matcher.Scratch scratch = new Matcher.Scratch();
        for (int pos = -1; pos < n; pos++) {
            Id[] fixed = new Id[n];
            if (pos >= 0) fixed[pos] = match[pos];
            String wo = pos < 0 ? "ohne feste Position" : "fest an Position " + pos;
            List<Id[]> found = Matcher.findMatchesWithFixed(g, vals, fwd, fixed);
            assertEquals(text(List.<Id[]>of(match)), text(found), wo);
            if (pos >= 0) {
                Matcher.findMatchesPlanned(g, vals, fwd, ix.plan(0, pos, fwd), pos, match[pos],
                        scratch);
                assertEquals(text(found), text(scratch.found), wo + ", verankerter Weg");
            }
        }
        Vergleich v = new Vergleich(g, vals, List.of(fwd));
        v.alle("junit5");
        v.engineGleich("junit5");
        assertTrue(v.nurNeu202 > 0, "2.0.2 findet den Match nicht");
        assertEquals(0, v.ohneWert202);
        vals.put(xn, "Test");
        assertEquals(0, Matcher.findMatchesWithFixed(g, vals, fwd, new Id[n]).size(),
                "verschiedene Werte");
    }

    /**
     * Der Graph zu {@link #JUNIT5_PLAN} mit einem Match. Rückgabe: der Match
     * in Musterfolge R, p, pn, cu, xn, Corr, P. {@code wertPn} und
     * {@code wertXn}: die Werte der beiden Blätter, null für keinen Eintrag
     * im Wertespeicher.
     */
    private static Id[] junit5Graph(Graph g, Map<Id, String> vals, String wertPn,
            String wertXn) {
        Id root = g.addBaseline("R", "tst:Root");
        Id p = g.addBaseline("p", "tst:Node");
        Id pn = g.addBaseline("pn", "name");
        Id project = g.addBaseline("P", "sl:Project");
        Id cu = g.addBaseline("cu", "sl:Node");
        Id xn = g.addBaseline("xn", "name");
        Id corr = g.addBaseline("c", "Ctestunit_Root");
        if (wertPn != null) vals.put(pn, wertPn);
        if (wertXn != null) vals.put(xn, wertXn);
        g.connect(root, p, St.SOLID);
        g.connect(p, pn, St.SOLID);
        g.connect(project, cu, St.SOLID);
        g.connect(cu, xn, St.SOLID);
        g.connect(corr, root, St.SOLID);
        g.connect(corr, project, St.SOLID);
        return new Id[] {root, p, pn, cu, xn, corr, project};
    }

    /**
     * Zwei Blätter vorname und rufname eines Knotens K, gleich über
     * {@code same_value_links}. Vorwärts die Positionen n, l1, l2 mit den
     * Links n → l1, n → l2 und der Wertgleichheit l1 = l2 dahinter.
     */
    private static final String ZWEI_BLAETTER = "{\"format\":3,\"name\":\"blaetter\",\"rules\":["
            + "{\"name\":\"ZweiBlaetter\",\"rank\":1,"
            + "\"left\":{\"anchor\":\"n\",\"nodes\":["
            + "{\"name\":\"n\",\"type\":\"K\"},"
            + "{\"name\":\"l1\",\"type\":\"vorname\"},"
            + "{\"name\":\"l2\",\"type\":\"rufname\"}],"
            + "\"links\":[[\"n\",\"l1\"],[\"n\",\"l2\"]],"
            + "\"same_value_links\":[[\"l1\",\"l2\"]]},"
            + "\"right\":{\"anchor\":\"m\",\"nodes\":[{\"name\":\"m\",\"type\":\"M\"}],"
            + "\"links\":[]},"
            + "\"corrs\":[{\"type\":\"CKM\",\"left\":\"n\",\"right\":\"m\","
            + "\"role\":\"establishes\",\"bindings\":[]}]}]}";

    /**
     * Zwei Blätter ohne Wert sind nicht gleich, wie in Rust
     * ({@code closing_links_ok}: {@code a.is_some() && a == …}). Java 2.0.2
     * ließ zwei fehlende Werte als gleich durch und fand einen Match mit
     * zwei Blättern ohne Wert an einer Wertgleichheit überall, außer wo die
     * Wertgleichheit der erste Link des Schritts war, der das zweite Blatt
     * setzt, und die beiden Blätter keine Nachbarn waren. Jetzt findet die
     * Suche einen solchen Match nie. Drei Lagen, die Erwartungen für 2.0.2
     * stehen über {@code Alt.find202}. Für {@link #ZWEI_BLAETTER} an allen
     * Stellen und für {@link #JUNIT5_PLAN} ohne feste Position gibt ein
     * Lauf gegen fa12974 mit denselben Regeln und Graphen dieselben
     * Matches:
     *
     * <ul>
     * <li>{@link #JUNIT5_PLAN} mit pn und xn ohne Wert. 2.0.2 erreichte xn
     *     nur über die Wertgleichheit, pn und xn sind keine Nachbarn, 2.0.2
     *     fand den Match nicht. Mit der Behebung im Plan allein ergäbe null
     *     gleich null einen Match, den weder 2.0.2 noch Rust findet.
     *     Erwartet wird kein Match, ohne feste Position, an jeder festen
     *     Position und auf dem verankerten Weg, ebenso mit nur einem der
     *     beiden Werte.
     * <li>Ein Muster nur aus der Wertgleichheit, die beiden Enden Nachbarn
     *     und ohne Wert: 2.0.2 fand den Match (Kandidat aus den Nachbarn),
     *     jetzt keinen.
     * <li>{@link #ZWEI_BLAETTER}: 2.0.2 erreichte das zweite Blatt über
     *     seinen Link vom Knoten und fand den Match ohne Werte an jeder
     *     Stelle, jetzt keinen. Mit gleichen Werten finden beide den einen
     *     Match, mit nur einem Wert oder zwei verschiedenen keinen.
     * </ul>
     */
    @Test
    void zweiFehlendeWerteSindNichtGleich() {
        String[][] werte = {{null, null}, {"Plan", null}, {null, "Plan"}};
        for (String[] w : werte) {
            Graph g = new Graph();
            Map<Id, String> vals = new HashMap<>();
            Id[] match = junit5Graph(g, vals, w[0], w[1]);
            Rule fwd = Rules.load(JUNIT5_PLAN, g).get(0);
            int n = match.length;
            AnchorIndex ix = AnchorIndex.build(List.of(fwd));
            Matcher.Scratch scratch = new Matcher.Scratch();
            for (int pos = -1; pos < n; pos++) {
                Id[] fixed = new Id[n];
                if (pos >= 0) fixed[pos] = match[pos];
                String wo = "pn " + w[0] + ", xn " + w[1]
                        + (pos < 0 ? ", ohne feste Position" : ", fest an Position " + pos);
                assertEquals("", text(Matcher.findMatchesWithFixed(g, vals, fwd, fixed)), wo);
                assertEquals("", text(Alt.find202(g, vals, fwd, fixed)), wo + ", 2.0.2");
                if (pos >= 0) {
                    Matcher.findMatchesPlanned(g, vals, fwd, ix.plan(0, pos, fwd), pos,
                            match[pos], scratch);
                    assertEquals("", text(scratch.found), wo + ", verankerter Weg");
                }
            }
            Vergleich v = new Vergleich(g, vals, List.of(fwd));
            v.alle("junit5 pn " + w[0] + ", xn " + w[1]);
            v.engineGleich("junit5 pn " + w[0] + ", xn " + w[1]);
            assertEquals(0, v.ohneWert202 + v.mitWerten202 + v.nurNeu202,
                    "junit5 pn " + w[0] + ", xn " + w[1] + ": 2.0.2 und die Suche ohne Match");
        }
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id u = g.addBaseline("u", "L1");
        Id w = g.addBaseline("w", "L2");
        g.connect(u, w, St.SOLID);
        Rule r = regel(g, "Wert", List.of("L1", "L2"), List.of(PatLink.sameValue(0, 1)));
        for (Id[] fixed : List.of(new Id[2], new Id[] {u, null}, new Id[] {null, w},
                new Id[] {u, w})) {
            String wo = "Nachbarn ohne Wert, fest " + text(Vergleich.fixed(fixed));
            assertEquals("", text(Matcher.findMatchesWithFixed(g, vals, r, fixed)), wo);
            assertEquals(text(List.<Id[]>of(new Id[] {u, w})),
                    text(Alt.find202(g, vals, r, fixed)), wo + ", 2.0.2");
        }
        Vergleich nachbarn = new Vergleich(g, vals, List.of(r));
        nachbarn.alle("Nachbarn ohne Wert");
        nachbarn.engineGleich("Nachbarn ohne Wert");
        assertTrue(nachbarn.ohneWert202 > 0, "Nachbarn ohne Wert: 2.0.2 fand den Match");
        assertEquals(0, nachbarn.mitWerten202 + nachbarn.nurNeu202);

        String[][] blaetter = {{null, null}, {"v", "v"}, {"v", null}, {null, "v"}, {"v", "x"}};
        for (String[] b : blaetter) {
            Graph gb = new Graph();
            Map<Id, String> vb = new HashMap<>();
            Id k = gb.addBaseline("n", "K");
            Id l1 = gb.addBaseline("a", "vorname");
            Id l2 = gb.addBaseline("b", "rufname");
            if (b[0] != null) vb.put(l1, b[0]);
            if (b[1] != null) vb.put(l2, b[1]);
            gb.connect(k, l1, St.SOLID);
            gb.connect(k, l2, St.SOLID);
            Rule zwei = Rules.load(ZWEI_BLAETTER, gb).get(0);
            assertEquals("ZweiBlaetter→", zwei.name);
            Id[] m = {k, l1, l2};
            boolean gleich = b[0] != null && b[0].equals(b[1]);
            boolean ohne = b[0] == null && b[1] == null;
            String eins = text(List.<Id[]>of(m));
            String soll = gleich ? eins : "";
            String soll202 = gleich || ohne ? eins : "";
            List<Id[]> stellen = new ArrayList<>();
            stellen.add(new Id[3]);
            for (int pos = 0; pos < 3; pos++) {
                Id[] fixed = new Id[3];
                fixed[pos] = m[pos];
                stellen.add(fixed);
            }
            stellen.add(m.clone());
            String welt = "zwei Blätter " + b[0] + "/" + b[1];
            for (Id[] fixed : stellen) {
                String wo = welt + ", fest " + text(Vergleich.fixed(fixed));
                assertEquals(soll, text(Matcher.findMatchesWithFixed(gb, vb, zwei, fixed)), wo);
                assertEquals(soll202, text(Alt.find202(gb, vb, zwei, fixed)), wo + ", 2.0.2");
            }
            Vergleich v = new Vergleich(gb, vb, List.of(zwei));
            v.alle(welt);
            v.engineGleich(welt);
            assertEquals(gleich, v.mitWerten202 > 0, welt + ": 2.0.2 mit Werten");
            assertEquals(ohne, v.ohneWert202 > 0, welt + ": 2.0.2 ohne Wert");
            assertEquals(0, v.nurNeu202, welt + ": nur neu");
        }
    }

    /**
     * {@link Matcher#linksOk} mit einer Liste des Hosts. Code 2 vergleicht
     * die Werte über {@code Matcher.sameValue} wie 2.0.2
     * ({@code Alt.linksOk} mit treu), gleich welche Verbindungen zwischen
     * den beiden Knoten bestehen. Die übrigen Codes prüfen die Richtung wie
     * bisher. Einzige Abweichung von 2.0.2: zwei fehlende Werte bestehen
     * nicht ({@link #zweiFehlendeWerteSindNichtGleich}). Für die Listen
     * aus einem Link und für Code 1 mit Code 2 gibt ein Lauf gegen fa12974
     * dieselben Werte wie {@code Alt.linksOk} mit treu.
     */
    @Test
    void linksOkMitCode2VergleichtDieWerte() {
        String[][] werte = {{"v", "v"}, {"v", "x"}, {"v", null}, {null, "v"}, {null, null}};
        int[][][] listen = {
            {{0, 2}}, {{0, 1}}, {{0, 0}}, {{0, -1}}, {{0, 1}, {0, 2}}, {{0, 2}, {0, 0}},
        };
        String[] verbindungen = {"keine", "u->w", "w->u"};
        int abweichend = 0;
        int geprueft = 0;
        for (int verb = 0; verb < verbindungen.length; verb++) {
            for (String[] w : werte) {
                Graph g = new Graph();
                Map<Id, String> vals = new HashMap<>();
                Id u = g.addBaseline("u", "L1");
                Id x = g.addBaseline("w", "L2");
                if (w[0] != null) vals.put(u, w[0]);
                if (w[1] != null) vals.put(x, w[1]);
                if (verb == 1) g.connect(u, x, St.SOLID);
                if (verb == 2) g.connect(x, u, St.SOLID);
                Id[] cur = {u, null};
                for (int[][] liste : listen) {
                    List<int[]> links = List.of(liste);
                    StringBuilder codes = new StringBuilder();
                    boolean code2 = false;
                    for (int[] l : liste) {
                        codes.append(' ').append(l[1]);
                        code2 |= l[1] == 2;
                    }
                    String wo = "Verbindung " + verbindungen[verb] + ", u " + w[0] + ", w " + w[1]
                            + ", Codes" + codes;
                    boolean alt = Alt.linksOk(g, vals, cur, links, x, true);
                    boolean neu = Matcher.linksOk(g, vals, cur, links, x);
                    geprueft++;
                    if (alt != neu) {
                        abweichend++;
                        assertTrue(code2 && w[0] == null && w[1] == null && alt && !neu,
                                wo + ": 2.0.2 " + alt + ", jetzt " + neu);
                    }
                }
                if (verb == 0 && "v".equals(w[0]) && "v".equals(w[1])) {
                    assertTrue(Matcher.linksOk(g, vals, cur, List.of(new int[][] {{0, 2}}), x),
                            "gleiche Werte ohne Verbindung");
                }
                if (verb > 0 && "x".equals(w[1])) {
                    assertTrue(!Matcher.linksOk(g, vals, cur, List.of(new int[][] {{0, 2}}), x),
                            "verschiedene Werte mit Verbindung " + verbindungen[verb]);
                }
            }
        }
        assertEquals(3 * 5 * 6, geprueft);
        // Zwei fehlende Werte: Code 2 allein in allen drei Lagen, Code 1
        // und 2 mit w->u, Code 2 und 0 mit u->w.
        assertEquals(5, abweichend);
    }

    /**
     * {@link #JUNIT5_PLAN} durch die Engine: der Seed über den Index
     * ({@code seedFactor = 0}), je Knoten des Matches {@code elementsAdded}
     * an einer frischen Engine und alle Knoten in einem Aufruf. Erwartet
     * wird je Engine genau der eine Match, lebend. Unter
     * {@code -Dseesaw.checks=true} ({@code jgate.sh mvn3}) vergleicht die
     * Engine dabei den Index mit der ungefilterten Suche. Ein Lauf gegen
     * fa12974 trägt an keiner dieser Stellen einen Match ein.
     */
    @Test
    void junit5PlanDurchDieEngine() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id[] match = junit5Graph(g, vals, "Plan", "Plan");
        List<Rule> rules = List.of(Rules.load(JUNIT5_PLAN, g).get(0));
        String soll = text(List.<Id[]>of(match));
        Engine seed = new Engine(rules, true);
        seed.seedFactor = 0;
        seed.seed(g, vals);
        assertEquals(soll, eingetragen(seed), "Seed über den Index");
        for (Id id : match) {
            Engine e = new Engine(rules, true);
            e.elementsAdded(g, vals, List.of(id));
            assertEquals(soll, eingetragen(e), "elementsAdded " + text(new Id[] {id}));
        }
        Engine alle = new Engine(rules, true);
        alle.elementsAdded(g, vals, List.of(match));
        assertEquals(soll, eingetragen(alle), "elementsAdded mit allen Knoten");
    }

    /** Die lebenden Matches einer Engine in der Folge von {@code matches},
     *  je Zeile die Ref-Folge, wie {@link #text(List)}. */
    private static String eingetragen(Engine e) {
        List<Id[]> lebend = new ArrayList<>();
        for (Engine.MatchRec m : e.matches) {
            assertEquals(0, m.ruleIx);
            if (!m.dead) lebend.add(m.refs);
        }
        assertEquals(e.matches.size(), lebend.size(), "tote Matches");
        return text(lebend);
    }

    /** Der Regelsatz langpack (k = 10) über 120 Sprachknoten: vor dem
     *  ersten Lauf, synchron, nach Hinzufügen, Ändern und Löschen. */
    @Test
    void langpackWieInJava202() {
        Langpack.Model m = new Langpack.Model(new Langpack.Spec(10, 120));
        List<Rule> rules = Rules.load(Langpack.rulesJson(10), m.g);
        Engine e = new Engine(rules, true);
        Vergleich v = new Vergleich(m.g, m.vs, rules);
        v.alle("langpack leer");
        e.elementsAdded(m.g, m.vs, m.all);
        saturate(e, m.g, m.vs);
        v.alle("langpack synchron");
        for (int i = 0; i < 4; i++) {
            List<Id> fresh = m.grow();
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementsAdded(m.g, m.vs, fresh);
            saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        v.alle("langpack hinzugefügt");
        List<Id> ruleLeaves = idsOfType(m.g, "rule");
        for (int i = 0; i < 4; i++) {
            Id leaf = ruleLeaves.get(1 + 7 * i);
            m.vs.put(leaf, "k" + ((i + 1) % 5));
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(leaf);
            e.elementChanged(m.g, leaf);
            e.elementsAdded(m.g, m.vs, List.of(leaf));
            saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        v.alle("langpack geändert");
        List<Id> lang = idsOfType(m.g, "J");
        for (int i = 0; i < 4; i++) {
            Id node = lang.get(3 + 11 * i);
            if (!m.g.node(node).status.matchable()) continue;
            m.g.setNodeStatus(node, St.TOMBSTONE);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(node);
            e.elementDeleted(m.g, node);
            saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        v.alle("langpack gelöscht");
        v.engineGleich("langpack");
        assertTrue(v.matches > 1000, "nur " + v.matches + " Matches");
    }

    /** Die Knoten eines Typs, nach Id sortiert. */
    private static List<Id> idsOfType(Graph g, String typ) {
        int t = g.lookup(typ);
        List<Id> out = new ArrayList<>();
        for (Node n : g.allNodes()) if (n.typ == t) out.add(n.id);
        out.sort(Id::compareTo);
        return out;
    }

    /** Der Regelsatz f2p über 40 Familien: vor dem ersten Lauf,
     *  synchron, nach Hinzufügen und nach Löschen von Mitgliedern. */
    @Test
    void f2pWieInJava202() {
        F2p.Model m = new F2p.Model(40);
        List<Rule> rules = Rules.load(F2p.rulesJson(), m.g);
        Engine e = new Engine(rules, true);
        Vergleich v = new Vergleich(m.g, m.vs, rules);
        v.alle("f2p leer");
        e.elementsAdded(m.g, m.vs, m.all);
        saturate(e, m.g, m.vs);
        v.alle("f2p synchron");
        int vorher = v.mitClosing;
        v.paare("f2p synchron, zwei feste");
        assertTrue(v.mitClosing > vorher, "keine Stelle mit Plan.closing");
        for (int i = 0; i < 3; i++) {
            List<Id> fresh = m.grow();
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementsAdded(m.g, m.vs, fresh);
            saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        v.alle("f2p hinzugefügt");
        for (int i = 0; i < 4; i++) {
            Id member = m.members.get(5 + 37 * i);
            m.g.setNodeStatus(member, St.TOMBSTONE);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(member);
            e.elementDeleted(m.g, member);
            saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        v.alle("f2p gelöscht");
        v.engineGleich("f2p");
        assertTrue(v.matches > 1000, "nur " + v.matches + " Matches");
    }
}
