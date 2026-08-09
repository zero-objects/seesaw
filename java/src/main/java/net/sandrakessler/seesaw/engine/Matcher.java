package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

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

    public static List<Step> buildPlan(Rule r, Id[] fixed) {
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
                } else if (l.to == next && placed[l.from]) {
                    s.links.add(new int[] {l.from, code == -9 ? 0 : code});
                }
            }
            placed[next] = true;
            steps.add(s);
        }
        return steps;
    }

    public static boolean nodeOk(Graph g, Map<Id, String> vals, PatNode pn, Id id) {
        Node n = g.node(id);
        if (n == null || !n.status.matchable() || n.typ != pn.typ) return false;
        return pn.predMatches(g.resolveValue(id, vals));
    }

    public static boolean linksOk(Graph g, Map<Id, String> vals, Id[] cur, List<int[]> links,
            Id cand) {
        for (int[] l : links) {
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
            Id s = cur[l.from];
            Id t = cur[l.to];
            if (s == null || t == null) return false;
            boolean ok;
            switch (l.kind) {
                case SAME_VALUE:
                    ok = sameValue(g, vals, s, t);
                    break;
                case CONTEXT:
                    ok = g.connected(s, t) || g.connected(t, s);
                    break;
                default:
                    ok = g.connected(s, t);
                    break;
            }
            if (!ok) return false;
        }
        return true;
    }

    public static void enumerate(Graph g, Map<Id, String> vals, Rule r, List<Step> plan, int depth,
            Id[] cur, List<Id[]> out, boolean[] foundAny, boolean stopFirst) {
        if (depth == plan.size()) {
            if (allLinksOk(g, vals, r, cur)) {
                foundAny[0] = true;
                if (out != null) out.add(cur.clone());
            }
            return;
        }
        Step step = plan.get(depth);
        PatNode pn = r.patNodes.get(step.nodeIx);
        if (step.preBound) {
            if (nodeOk(g, vals, pn, cur[step.nodeIx]))
                enumerate(g, vals, r, plan, depth + 1, cur, out, foundAny, stopFirst);
            return;
        }
        List<Id> candidates = new ArrayList<>();
        if (!step.links.isEmpty()) {
            int[] first = step.links.get(0);
            Id anchor = cur[first[0]];
            for (Part p : g.partsByOtherType(anchor, pn.typ)) {
                if (first[1] == 1 && p.outgoing) continue;      // cand ist source ⇒ part incoming
                if (first[1] == 0 && !p.outgoing) continue;     // cand ist target ⇒ part outgoing
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
            enumerate(g, vals, r, plan, depth + 1, cur, out, foundAny, stopFirst);
            cur[step.nodeIx] = null;
            if (stopFirst && foundAny[0]) return;
        }
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
        List<Step> plan = buildPlan(r, fixed);
        Id[] cur = fixed.clone();
        List<Id[]> out = new ArrayList<>();
        enumerate(g, vals, r, plan, 0, cur, out, new boolean[1], false);
        out.sort(Matcher::compareBindings);
        return out;
    }

// ── Matcher (Spiegel von matcher.rs) ──
static final class Step {
    public int nodeIx;
    public boolean preBound;
    final List<int[]> links = new ArrayList<>(); // [placedPos, dir] dir: 1=candIsSource,0=candIsTarget,-1=context
}
}
