package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import java.util.ArrayList;
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
 * als {@link Alt} kopiert.
 *
 * <p>Verglichen wird je Stelle (Regel, feste Positionen) die Menge der
 * Matches nach {@link Matcher#compareBindings}: dieselben Bindungen,
 * jede höchstens so oft wie in 2.0.2 und mindestens einmal. Weniger oft
 * nur dort, wo ein richtungsfreier Link der erste Link eines Schritts
 * ist und nur eine der beiden Verbindungen zwischen den Knoten matchbar
 * ist (Rust 891a63b). Dazu die Wirkung auf die Engine: beide Folgen von
 * Matches werden in je eine frische Engine eingetragen, deren Zustand
 * (Matches, Schlüssel, {@code byElement} in seiner Reihenfolge, To-do)
 * gleich sein muss.
 */
final class MatcherTest {

    // ── Die Suche von Java 2.0.2 (Matcher und Graph.connected), unverändert ──

    private static final class Alt {
        static final class Step {
            int nodeIx;
            boolean preBound;
            final List<int[]> links = new ArrayList<>();
        }

        static List<Step> buildPlan(Rule r, Id[] fixed) {
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
                        if ((l.from == i && placed[l.to]) || (l.to == i && placed[l.from])) {
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
                Id cand) {
            for (int[] l : links) {
                Id other = cur[l[0]];
                boolean ok;
                if (l[1] == 2) {
                    ok = sameValue(g, vals, cand, other);
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

        static boolean sameValue(Graph g, Map<Id, String> vals, Id a, Id b) {
            String va = g.resolveValue(a, vals);
            String vb = g.resolveValue(b, vals);
            return va == null ? vb == null : va.equals(vb);
        }

        static boolean allLinksOk(Graph g, Map<Id, String> vals, Rule r, Id[] cur) {
            for (PatLink l : r.patLinks) {
                Id s = cur[l.from];
                Id t = cur[l.to];
                if (s == null || t == null) return false;
                boolean ok;
                switch (l.kind) {
                    case SAME_VALUE:
                        ok = sameValue(g, vals, s, t);
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
                Id[] cur, List<Id[]> out) {
            if (depth == plan.size()) {
                if (allLinksOk(g, vals, r, cur)) out.add(cur.clone());
                return;
            }
            Step step = plan.get(depth);
            PatNode pn = r.patNodes.get(step.nodeIx);
            if (step.preBound) {
                if (nodeOk(g, vals, pn, cur[step.nodeIx]))
                    enumerate(g, vals, r, plan, depth + 1, cur, out);
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
                if (!linksOk(g, vals, cur, step.links, cand)) continue;
                cur[step.nodeIx] = cand;
                enumerate(g, vals, r, plan, depth + 1, cur, out);
                cur[step.nodeIx] = null;
            }
        }

        static List<Id[]> find(Graph g, Map<Id, String> vals, Rule r, Id[] fixed) {
            List<Step> plan = buildPlan(r, fixed);
            Id[] cur = fixed.clone();
            List<Id[]> out = new ArrayList<>();
            enumerate(g, vals, r, plan, 0, cur, out);
            out.sort(Matcher::compareBindings);
            return out;
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

    /** Zustand einer Engine, die nur eingetragen hat: Matches, Schlüssel,
     *  byElement in seiner Reihenfolge, To-do. */
    private static String zustand(Engine e) {
        StringBuilder sb = new StringBuilder();
        for (Engine.MatchRec m : e.matches) {
            sb.append("m ").append(Engine.key(m.ruleIx, m.refs)).append(' ').append(m.dead)
                    .append(' ').append(m.entry).append('\n');
        }
        for (Map.Entry<String, Integer> k : new TreeMap<>(e.byKey).entrySet()) {
            sb.append("k ").append(k.getKey()).append(' ').append(k.getValue()).append('\n');
        }
        for (Map.Entry<Id, ArrayList<Integer>> x : e.byElement.entrySet()) {
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
            suchen++;
            matches += n.size();
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
                    + " weggefallen=" + weg + " eingetragen=" + neu.matches.size());
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
     * {@link Graph#connectionId} liest die Id aus der Participation-Liste
     * der Quelle, als Array und als TreeSet (mehr als 48 Einträge): für
     * jedes geordnete Paar gleich der Ableitung, wo die Verbindung
     * besteht, sonst null; connected und connectedAlive wie in 2.0.2.
     */
    @Test
    void verbindungAusDerParticipationListe() {
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

    private static Id blatt(Graph g, Map<Id, String> vals, String name, String typ, String wert) {
        Id id = g.addBaseline(name, typ);
        vals.put(id, wert);
        return id;
    }

    /**
     * Wertgleichheit als erster Link eines Schritts: die Kandidaten
     * kommen wie in 2.0.2 aus den Participations des gesetzten Knotens,
     * in jeder Richtung und ohne Blick auf den Status der Verbindung. Ein
     * Knoten mit gleichem Wert, der kein Nachbar ist, wird nicht gefunden,
     * einer über eine Verbindung im Tombstone schon, ein Nachbar über
     * beide Richtungen zweimal. Sind beide Positionen fest, prüft die
     * Abschlussprüfung den Wert.
     */
    @Test
    void wertgleichheitAlsErsterLinkEinesSchrittsWieInJava202() {
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
        List<Id[]> ab = List.of(new Id[] {a, b}, new Id[] {a, b}, new Id[] {a, e});
        ArrayList<Id[]> erwartet = new ArrayList<>(ab);
        erwartet.sort(Matcher::compareBindings);
        for (Id[] fixed : List.of(new Id[] {a, null}, new Id[2])) {
            assertEquals(text(erwartet), text(Alt.find(g, vals, r, fixed)), "2.0.2");
            assertEquals(text(erwartet), text(Matcher.findMatchesWithFixed(g, vals, r, fixed)));
        }
        Id[][][] stellen = {
            {{null, b}, {a, b}, {a, b}},
            {{null, d}},
            {{a, d}, {a, d}},
            {{a, c}},
        };
        for (Id[][] s : stellen) {
            List<Id[]> soll = new ArrayList<>();
            for (int i = 1; i < s.length; i++) soll.add(s[i]);
            assertEquals(text(soll), text(Alt.find(g, vals, r, s[0])), "2.0.2");
            assertEquals(text(soll), text(Matcher.findMatchesWithFixed(g, vals, r, s[0])));
        }
        Vergleich v = new Vergleich(g, vals, List.of(r));
        v.alle("wert");
        assertEquals(0, v.weg);
        v.engineGleich("wert");
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
