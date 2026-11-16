package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;

/** Match-Enumeration ueber die Beteiligungslisten des Graphen. */
public final class Matcher {
    private Matcher() {}

    /**
     * Suchplan eines Musters für eine Menge vorab gebundener Positionen
     * (Rust {@code Plan}): die Schritte und die Links, die kein Schritt
     * prüft.
     */
    static final class Plan {
        final List<Step> steps;
        /**
         * Die Links, die kein Schritt prüft, als Indizes in
         * {@code Rule.patLinks}: Selbstlinks und Links zwischen zwei vorab
         * gebundenen Positionen. Jeden anderen Link prüft der Schritt, der
         * das spätere seiner beiden Enden setzt, mit derselben Bedingung
         * wie {@link #allLinksOk}. Anders als Rust gehört dazu auch die
         * Wertgleichheit: Java 2.0.2 nimmt sie in den Plan (Code 2), der
         * Schritt prüft sie mit {@code sameValue} wie die Abschlussprüfung.
         */
        final int[] closing;

        Plan(List<Step> steps, int[] closing) {
            this.steps = steps;
            this.closing = closing;
        }
    }

    public static List<Step> buildPlan(Rule r, Id[] fixed) {
        return plan(r, fixed).steps;
    }

    /** Der Plan für die festen Positionen {@code fixed}, samt den Links,
     *  die kein Schritt prüft. */
    static Plan plan(Rule r, Id[] fixed) {
        int n = r.patNodes.size();
        boolean[] placed = new boolean[n];
        boolean[] inStep = new boolean[r.patLinks.size()];
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
            for (int li = 0; li < r.patLinks.size(); li++) {
                PatLink l = r.patLinks.get(li);
                // Kodierung je Bedingung: 1 = Kandidat -> anderer,
                // 0 = anderer -> Kandidat, -1 = irgendeine Richtung,
                // 2 = Wert-Gleichheit.
                int code;
                if (l.kind == PatLink.Kind.SAME_VALUE) {
                    code = 2;
                } else if (l.kind == PatLink.Kind.CONTEXT) {
                    code = -1;
                } else {
                    code = -9; // wird unten je nach Seite gesetzt
                }
                if (l.from == next && placed[l.to]) {
                    s.links.add(new int[] {l.to, code == -9 ? 1 : code});
                    inStep[li] = true;
                } else if (l.to == next && placed[l.from]) {
                    s.links.add(new int[] {l.from, code == -9 ? 0 : code});
                    inStep[li] = true;
                }
            }
            placed[next] = true;
            steps.add(s);
        }
        int m = 0;
        int[] closing = new int[inStep.length];
        for (int li = 0; li < inStep.length; li++) if (!inStep[li]) closing[m++] = li;
        return new Plan(steps, Arrays.copyOf(closing, m));
    }

    public static boolean nodeOk(Graph g, Map<Id, String> vals, PatNode pn, Id id) {
        Node n = g.node(id);
        if (n == null || !n.status.matchable() || n.typ != pn.typ) return false;
        return pn.predMatches(g.resolveValue(id, vals));
    }

    public static boolean linksOk(Graph g, Map<Id, String> vals, Id[] cur, List<int[]> links,
            Id cand) {
        return linksOk(g, vals, cur, links, 0, cand);
    }

    /** Die Links {@code links} ab dem Index {@code from}. */
    private static boolean linksOk(Graph g, Map<Id, String> vals, Id[] cur, List<int[]> links,
            int from, Id cand) {
        for (int i = from, k = links.size(); i < k; i++) {
            int[] l = links.get(i);
            Id other = cur[l[0]];
            boolean ok;
            if (l[1] == 2) {
                ok = sameValue(g, vals, cand, other);
            } else if (l[1] == 1) {
                ok = g.connected(cand, other);
            } else if (l[1] == 0) {
                ok = g.connected(other, cand);
            } else {
                ok = g.connected(cand, other) || g.connected(other, cand);
            }
            if (!ok) return false;
        }
        return true;
    }

    /** Wert-Gleichheit zweier Blaetter; null gleicht nur null. */
    private static boolean sameValue(Graph g, Map<Id, String> vals, Id a, Id b) {
        String va = g.resolveValue(a, vals);
        String vb = g.resolveValue(b, vals);
        return va == null ? vb == null : va.equals(vb);
    }

    public static boolean allLinksOk(Graph g, Map<Id, String> vals, Rule r, Id[] cur) {
        for (PatLink l : r.patLinks) {
            if (!linkOk(g, vals, l, cur)) return false;
        }
        return true;
    }

    /** Die Links von {@code plan.closing} auf der vollständigen Bindung. */
    private static boolean closingLinksOk(Graph g, Map<Id, String> vals, Rule r, Plan plan,
            Id[] cur) {
        for (int li : plan.closing) {
            if (!linkOk(g, vals, r.patLinks.get(li), cur)) return false;
        }
        return true;
    }

    private static boolean linkOk(Graph g, Map<Id, String> vals, PatLink l, Id[] cur) {
        Id s = cur[l.from];
        Id t = cur[l.to];
        if (s == null || t == null) return false;
        switch (l.kind) {
            case SAME_VALUE:
                return sameValue(g, vals, s, t);
            case CONTEXT:
                return g.connected(s, t) || g.connected(t, s);
            default:
                return g.connected(s, t);
        }
    }

    private static boolean matchable(Conn c) {
        return c != null && c.status.matchable();
    }

    /**
     * Die Suche ab Schritt {@code depth} eines Plans aus {@link #buildPlan}.
     * Am Ende werden alle Links des Musters geprüft, wie bisher: ein
     * Aufrufer kann die Positionen vor {@code depth} selbst gesetzt haben.
     */
    public static void enumerate(Graph g, Map<Id, String> vals, Rule r, List<Step> plan, int depth,
            Id[] cur, List<Id[]> out, boolean[] foundAny, boolean stopFirst) {
        int[] all = new int[r.patLinks.size()];
        for (int i = 0; i < all.length; i++) all[i] = i;
        enumerate(g, vals, r, new Plan(plan, all), depth, cur, out, foundAny, stopFirst);
    }

    /** {@code foundAny} darf null sein, wenn {@code stopFirst} nicht gilt. */
    private static void enumerate(Graph g, Map<Id, String> vals, Rule r, Plan plan, int depth,
            Id[] cur, List<Id[]> out, boolean[] foundAny, boolean stopFirst) {
        List<Step> steps = plan.steps;
        if (depth == steps.size()) {
            if (closingLinksOk(g, vals, r, plan, cur)) {
                if (foundAny != null) foundAny[0] = true;
                if (out != null) out.add(cur.clone());
            }
            return;
        }
        Step step = steps.get(depth);
        PatNode pn = r.patNodes.get(step.nodeIx);
        if (step.preBound) {
            if (nodeOk(g, vals, pn, cur[step.nodeIx]))
                enumerate(g, vals, r, plan, depth + 1, cur, out, foundAny, stopFirst);
            return;
        }
        // Die Kandidaten werden beim Lesen geprüft, ohne sie vorher zu
        // sammeln, in derselben Reihenfolge. Der Graph ändert sich
        // während einer Suche nicht.
        List<int[]> links = step.links;
        if (links.isEmpty()) {
            // Ohne Link zu einer gesetzten Position: jeder Knoten des
            // Typs (nodeOk verwirft die nicht matchbaren).
            List<Id> ids = g.idsOfType(pn.typ);
            for (int i = 0, k = ids.size(); i < k; i++) {
                if (tryCandidate(g, vals, r, plan, depth, cur, out, foundAny, stopFirst, step, pn,
                        ids.get(i), null, 0)) {
                    return;
                }
            }
            return;
        }
        // Kandidaten: die Participation-Liste des ersten gesetzten
        // Nachbarn, über den Bereich seines Typs. Bei einem gerichteten
        // oder richtungsfreien Link bringt der Kandidat die Verbindung
        // mit, die den ersten Link verwirklicht: deren Status wird dort
        // gelesen, linksOk prüft die übrigen Links. Bei einer
        // Wertgleichheit (Code 2) bleiben die Kandidaten die von 2.0.2
        // (jede Richtung, ohne Blick auf die Verbindung), und linksOk
        // prüft auch den ersten Link.
        int[] first = links.get(0);
        Id anchor = cur[first[0]];
        int code = first[1];
        int rest = code == 2 ? 0 : 1;
        for (Part p : g.partsOfType(anchor, pn.typ)) {
            if (code == 1 && p.outgoing) continue;      // cand ist source ⇒ part incoming
            if (code == 0 && !p.outgoing) continue;     // cand ist target ⇒ part outgoing
            if (tryCandidate(g, vals, r, plan, depth, cur, out, foundAny, stopFirst, step, pn,
                    p.other, code == 2 ? null : p.connection, rest)) {
                return;
            }
        }
    }

    /**
     * Prüft {@code cand} für den Schritt {@code depth} und sucht mit ihm
     * weiter. {@code via}: die Verbindung, die den ersten Link
     * verwirklicht (null: keine). {@code from}: der erste Link, den
     * linksOk prüft. Rückgabe true: anhalten (erster Treffer gesucht und
     * gefunden).
     */
    private static boolean tryCandidate(Graph g, Map<Id, String> vals, Rule r, Plan plan,
            int depth, Id[] cur, List<Id[]> out, boolean[] foundAny, boolean stopFirst, Step step,
            PatNode pn, Id cand, Id via, int from) {
        for (Id c : cur) if (cand.equals(c)) return false; // injektiv
        if (via != null && !matchable(g.conn(via))) return false;
        if (!nodeOk(g, vals, pn, cand)) return false;
        if (!linksOk(g, vals, cur, step.links, from, cand)) return false;
        cur[step.nodeIx] = cand;
        enumerate(g, vals, r, plan, depth + 1, cur, out, foundAny, stopFirst);
        cur[step.nodeIx] = null;
        return stopFirst && foundAny[0];
    }

    public static int compareBindings(Id[] a, Id[] b) {
        int n = Math.min(a.length, b.length);
        for (int i = 0; i < n; i++) {
            int c = a[i].compareTo(b[i]);
            if (c != 0) return c;
        }
        return Integer.compare(a.length, b.length);
    }

    public static List<Id[]> findMatchesWithFixed(Graph g, Map<Id, String> vals, Rule r, Id[] fixed) {
        Plan plan = plan(r, fixed);
        Id[] cur = fixed.clone();
        List<Id[]> out = new ArrayList<>();
        enumerate(g, vals, r, plan, 0, cur, out, null, false);
        out.sort(Matcher::compareBindings);
        return out;
    }

    /**
     * Arbeitsspeicher der verankerten Suchen (Rust {@code Scratch}): die
     * Bindung und die Liste der Matches. Die Engine behält ihn über die
     * Suchen, eine Suche legt dann nur die Matches selbst an.
     */
    static final class Scratch {
        /** An Stelle n eine Bindung der Länge n, für Muster mit n Knoten. */
        private Id[][] cur = new Id[0][];
        /** Die Matches der letzten {@link #findMatchesPlanned}, in
         *  mu-Ordnung. Gilt bis zur nächsten Suche. */
        final ArrayList<Id[]> found = new ArrayList<>();

        /** Eine leere Bindung der Länge {@code n}. */
        Id[] cur(int n) {
            if (n >= cur.length) cur = Arrays.copyOf(cur, n + 1);
            Id[] c = cur[n];
            if (c == null) cur[n] = c = new Id[n];
            else Arrays.fill(c, null);
            return c;
        }
    }

    /**
     * Die Suche mit {@code id} an Position {@code pos} und sonst keiner
     * festen Position, entlang eines Plans, der für genau diese Position
     * gebaut ist ({@link AnchorIndex#plan}): der Plan hängt nur daran,
     * WELCHE Positionen fest sind, nicht an ihren Knoten. Die Matches
     * stehen danach in {@code scratch.found}, in derselben Folge wie bei
     * {@link #findMatchesWithFixed}.
     */
    static void findMatchesPlanned(Graph g, Map<Id, String> vals, Rule r, Plan plan, int pos, Id id,
            Scratch scratch) {
        Id[] cur = scratch.cur(r.patNodes.size());
        cur[pos] = id;
        ArrayList<Id[]> found = scratch.found;
        found.clear();
        enumerate(g, vals, r, plan, 0, cur, found, null, false);
        if (found.size() > 1) found.sort(Matcher::compareBindings);
    }

// ── Matcher (Spiegel von matcher.rs) ──
static final class Step {
    public int nodeIx;
    public boolean preBound;
    final List<int[]> links = new ArrayList<>(); // [placedPos, dir] dir: 1=candIsSource,0=candIsTarget,-1=context,2=Wert-Gleichheit
}
}
