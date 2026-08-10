package net.sandrakessler.seesaw.engine;

import net.sandrakessler.seesaw.ident.Ident;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.Rule;

// ── Engine (Spiegel von engine.rs, Minimal-Pfad) ──
public final class Engine {
    public final List<Rule> rules;

    static final class TodoKey implements Comparable<TodoKey> {
        final long rank;
        final Id[] refs;
        final int ruleIx;

        TodoKey(long rank, Id[] refs, int ruleIx) {
            this.rank = rank; this.refs = refs; this.ruleIx = ruleIx;
        }

        @Override public int compareTo(TodoKey o) {
            // (Reverse(rank), Reverse(refs), ruleIx)
            int c = Long.compare(o.rank, rank);
            if (c != 0) return c;
            c = Matcher.compareBindings(o.refs, refs);
            if (c != 0) return c;
            return Integer.compare(ruleIx, o.ruleIx);
        }
    }

    static final class AppliedKey implements Comparable<AppliedKey> {
        final int ruleIx;
        final Id[] refs;

        AppliedKey(int r, Id[] refs) { ruleIx = r; this.refs = refs; }

        @Override public int compareTo(AppliedKey o) {
            int c = Integer.compare(ruleIx, o.ruleIx);
            if (c != 0) return c;
            return Matcher.compareBindings(refs, o.refs);
        }
    }

    /** Cascade-Entry (Spiegel von engine.rs::Entry). */
    public static final class Entry {
        public final int ruleIx;
        public final long rank;
        public final Id[] refs;
        public final List<Id> created;
        /** Erzeugte Kanten (nur fresh) — getrennt zurückgezogen (keine Provenienz-Kinder). */
        public final List<Id> createdEdges;

        Entry(int ruleIx, long rank, Id[] refs, List<Id> created, List<Id> createdEdges) {
            this.ruleIx = ruleIx; this.rank = rank; this.refs = refs;
            this.created = created; this.createdEdges = createdEdges;
        }
    }

    static final class MatchRec {
        final int ruleIx;
        final Id[] refs;
        boolean dead;
        /** Provenienz-Kante Match → Kaskaden-Eintrag (null = nicht angewandt). */
        Integer entry;

        MatchRec(int ruleIx, Id[] refs) { this.ruleIx = ruleIx; this.refs = refs; }
    }

    /** Sättigungs-Verdikt (Spiegel von engine::Termination). */
    public enum Termination { DUPLICATION, CONVERGENCE, STEP_LIMIT, CONTRADICTION }

    /** Backtracking-Schranke (rank, refs) — Spiegel von SelectionBound. */
    public static final class SelectionBound {
        final long rank;
        final Id[] refs;

        SelectionBound(long rank, Id[] refs) { this.rank = rank; this.refs = refs; }
    }

    final TreeSet<TodoKey> todo = new TreeSet<>();
    final TreeSet<AppliedKey> applied = new TreeSet<>();
    final HashMap<String, Integer> byKey = new HashMap<>();
    final ArrayList<MatchRec> matches = new ArrayList<>();
    final HashMap<Id, ArrayList<Integer>> byElement = new HashMap<>();
    public final ArrayList<Entry> cascade = new ArrayList<>();
    public int cascadeLen = 0;
    public boolean sawContradiction = false;
    /** Von der Retraktion gesammelte TT — consolidate arbeitet NUR diese ab (O(Δ)). */
    final ArrayList<Id> pendingTtNodes = new ArrayList<>();
    final ArrayList<Id> pendingTtEdges = new ArrayList<>();

    public Engine(List<Rule> rules) { this.rules = rules; }

    /**
     * Schluessel eines Matches: Regelindex plus Referenzfolge.
     *
     * <p>Feste Breite je Byte. {@code Integer.toHexString} laesst
     * fuehrende Nullen weg, damit war die Kodierung nicht injektiv:
     * die Folgen {@code 01 23} und {@code 12 03} ergaben beide
     * {@code "123"}, und {@link #record} konnte einen berechtigten
     * Match als Duplikat verwerfen. Rust nutzt die Referenzfolge
     * direkt als Schluessel, ohne Kodierung -- Java kann also nur
     * dann dieselben Entscheidungen treffen, wenn die Kodierung
     * umkehrbar ist. Im Review vom 2026-08-10 gefunden.
     */
    public static String key(int ruleIx, Id[] refs) {
        StringBuilder sb = new StringBuilder().append(ruleIx);
        for (Id r : refs) {
            sb.append(':');
            for (byte b : r.b) {
                sb.append(HEX[(b >> 4) & 0xF]).append(HEX[b & 0xF]);
            }
        }
        return sb.toString();
    }

    private static final char[] HEX = "0123456789abcdef".toCharArray();

    public void record(int ruleIx, Id[] refs) {
        String k = key(ruleIx, refs);
        if (byKey.containsKey(k)) return;
        int ix = matches.size();
        for (Id id : refs) byElement.computeIfAbsent(id, x -> new ArrayList<>()).add(ix);
        byKey.put(k, ix);
        matches.add(new MatchRec(ruleIx, refs));
        todo.add(new TodoKey(rules.get(ruleIx).rank, refs, ruleIx));
    }

    /** delete/modify (Spec §1.6): Matches des Elements EAGER töten. */
    public void elementRemoved(Id id) {
        ArrayList<Integer> refs = byElement.get(id);
        if (refs == null) return;
        for (int mix : refs) {
            MatchRec m = matches.get(mix);
            if (!m.dead) {
                m.dead = true;
                todo.remove(new TodoKey(rules.get(m.ruleIx).rank, m.refs, m.ruleIx));
            }
        }
    }

    /** Ein Match verliert seine Begründung (Kern von retractFor/linkRemoved):
     *  entkräften (dead + To-do), Vermerk vergessen (applied/byKey → Reclaim),
     *  und — falls angewandt — den Erzeugnissen über die Provenienz-Kante
     *  folgen (TT + Queue). Spiegel von retract_match. */
    public void retractMatch(Graph g, int mix, ArrayList<Id> queue) {
        MatchRec m = matches.get(mix);
        int ruleIx = m.ruleIx;
        Id[] refs = m.refs;
        Integer entry = m.entry;
        if (!m.dead) {
            m.dead = true;
            todo.remove(new TodoKey(rules.get(ruleIx).rank, refs, ruleIx));
        }
        // Reclaim-Fähigkeit: Vermerk VERGESSEN, damit eine identische
        // Re-Ableitung erneut anwenden (resurrektieren) kann.
        applied.remove(new AppliedKey(ruleIx, refs));
        byKey.remove(key(ruleIx, refs));
        if (entry != null) {
            Entry e = cascade.get(entry);
            for (Id c : e.created) {
                // Erzeugnis GENAU DIESES Eintrags, die Herkunft ist
                // durch die Schleife bewiesen -- der Status entscheidet
                // nicht.
                //
                // Bis 2026-08-10 stand hier == GHOST, und damit endete
                // die Retraktion an einer Materialisierung: ein
                // gefaltetes Erzeugnis ist SOLID und blieb stehen, das
                // Delta setzte also gar keinen Tombstone. Kriterium ist
                // die Herkunft, nicht der Lebenszyklus-Zustand; was
                // ueber addBaseline kam, steht in keinem created.
                //
                // TENTATIV, nicht endgueltig: die Konsolidierung am
                // Ende des Laufs entscheidet. Im selben Lauf neu
                // abgeleitet heisst reklamiert, sonst loest es zu
                // TOMBSTONE auf. Im Ruhezustand bleiben nur GHOST und
                // SOLID.
                Node n = g.node(c);
                if (n != null && n.status.matchable()) {
                    g.setNodeStatus(c, St.TENTATIVE_TOMBSTONE);
                    pendingTtNodes.add(c);
                }
                queue.add(c);
            }
            for (Id ed : e.createdEdges) {
                // Dieselbe Begruendung wie oben fuer die Knoten.
                Conn c = g.conn(ed);
                if (c != null && c.status.matchable()) {
                    g.setConnectionStatus(ed, St.TENTATIVE_TOMBSTONE);
                    pendingTtEdges.add(ed);
                }
            }
        }
    }

    /** Provenienz-Walk über die Queue (Spiegel von drain_retraction). */
    public void drainRetraction(Graph g, ArrayList<Id> queue) {
        TreeSet<Id> seen = new TreeSet<>();
        while (!queue.isEmpty()) {
            Id id = queue.remove(queue.size() - 1);
            if (!seen.add(id)) continue;
            ArrayList<Integer> mixes = byElement.get(id);
            if (mixes == null) continue;
            for (int mix : new ArrayList<>(mixes)) retractMatch(g, mix, queue);
        }
    }

    /** Retraction (M5.3/M5.5): ein Element ist weggefallen. */
    public void retractFor(Graph g, Id removed) {
        ArrayList<Id> queue = new ArrayList<>();
        queue.add(removed);
        drainRetraction(g, queue);
    }

    /** Kanten-Wegfall (Spiegel von link_removed): Matches, deren Refs
     *  BEIDE Endpunkte enthalten, verlieren ihre Begründung. */
    public void linkRemoved(Graph g, Id a, Id b) {
        ArrayList<Integer> inA = byElement.get(a);
        if (inA == null) return;
        ArrayList<Integer> inB = byElement.get(b);
        if (inB == null) return;
        TreeSet<Integer> setB = new TreeSet<>(inB);
        ArrayList<Id> queue = new ArrayList<>();
        for (int mix : inA) if (setB.contains(mix)) retractMatch(g, mix, queue);
        drainRetraction(g, queue);
    }

    /** Konsolidierung (M5.5): gesammelte TT → Tombstone (O(Δ), inkl.
     *  Kanten). Rückgabe (additiv seit E3, Rust gibt () zurück):
     *  Anzahl finalisierter Elemente = Eliminations-Zählung für den
     *  fold-Report. */
    public int consolidate(Graph g) {
        int eliminated = 0;
        for (Id id : pendingTtNodes) {
            Node n = g.node(id);
            if (n != null && n.status == St.TENTATIVE_TOMBSTONE) {
                g.setNodeStatus(id, St.TOMBSTONE);
                eliminated++;
            }
        }
        for (Id id : pendingTtEdges) {
            Conn c = g.conn(id);
            if (c != null && c.status == St.TENTATIVE_TOMBSTONE) {
                g.setConnectionStatus(id, St.TOMBSTONE);
                eliminated++;
            }
        }
        pendingTtNodes.clear();
        pendingTtEdges.clear();
        return eliminated;
    }

    public void seedRule(Graph g, Map<Id, String> vals, int ri) {
        Rule r = rules.get(ri);
        Id[] fixed = new Id[r.patNodes.size()];
        for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) record(ri, m);
    }

    public void seed(Graph g, Map<Id, String> vals) {
        for (int ri = 0; ri < rules.size(); ri++) seedRule(g, vals, ri);
    }

    /** Δ-Routing (Spiegel von seed_routed): Regel aktiv, wenn das Delta
     *  einen ihrer Eingangs-Typen berührt. Richtung wohnt im Delta. */
    public void seedRouted(Graph g, Map<Id, String> vals, List<String> deltaTypes) {
        for (int ri = 0; ri < rules.size(); ri++) {
            boolean hit = false;
            for (String t : rules.get(ri).inputTypes)
                if (deltaTypes.contains(t)) { hit = true; break; }
            if (hit) seedRule(g, vals, ri);
        }
    }

    /** Add-Strom (Spiegel von elements_added): extern hinzugefügte
     *  Elemente delta-lokal ankern; danach step bis Sättigung. */
    public void elementsAdded(Graph g, Map<Id, String> vals, List<Id> newNodes) {
        expandAt(g, vals, newNodes);
    }

    public void expandAt(Graph g, Map<Id, String> vals, List<Id> newNodes) {
        for (int ri = 0; ri < rules.size(); ri++) {
            Rule r = rules.get(ri);
            for (Id id : newNodes) {
                Node n = g.node(id);
                if (n == null) continue;
                for (int pos = 0; pos < r.patNodes.size(); pos++) {
                    if (r.patNodes.get(pos).typ != n.typ) continue;
                    Id[] fixed = new Id[r.patNodes.size()];
                    fixed[pos] = id;
                    for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) record(ri, m);
                }
            }
        }
    }

    /** null = dynamisches Blatt ohne Quelle (würde entfallen). */
    public Id previewCreate(Graph g, Rule r, Id[] refs, List<Id> ids, CreateNode cn) {
        Id parent = cn.parent >= 0 ? refs[cn.parent] : ids.get(-cn.parent - 1);
        if (cn.corrFullMatch) return Ident.identCorr(parent, cn.typ, refs);
        if (cn.dynAttr != null) {
            Id src = g.childLeafOfType(refs[cn.dynAnchor], cn.dynAttr);
            if (src == null) return null;
            return Ident.identDerived(parent, cn.typ, src, cn.dynTransform);
        }
        if (cn.konst != null) return Ident.identKonst(parent, cn.typ, r.name, ids.size());
        if (cn.derivedLeaf < 0) return Ident.identGhost(parent, cn.typ);
        return Ident.identDerived(parent, cn.typ, refs[cn.derivedLeaf], cn.derivedTransform);
    }

    public boolean creationExists(Graph g, Rule r, Id[] refs) {
        List<Id> ids = new ArrayList<>();
        for (CreateNode cn : r.createNodes) {
            Id id = previewCreate(g, r, refs, ids, cn);
            if (id == null) { // dyn ohne Quelle: nichts würde erzeugt
                ids.add(cn.parent >= 0 ? refs[cn.parent] : ids.get(-cn.parent - 1));
                continue;
            }
            Node n = g.node(id);
            if (n == null || !n.status.alive()) return false;
            ids.add(id);
        }
        for (int[] l : r.createLinks) {
            Id s = l[0] >= 0 ? refs[l[0]] : ids.get(-l[0] - 1);
            Id t = l[1] >= 0 ? refs[l[1]] : ids.get(-l[1] - 1);
            if (!g.connectedAlive(s, t)) return false;
        }
        return true;
    }

    static final class Created {
        final List<Id> nodes, edges;
        Created(List<Id> nodes, List<Id> edges) { this.nodes = nodes; this.edges = edges; }
    }

    public Created applyCreation(Graph g, Rule r, Id[] refs) {
        // Plan-indiziert; null-Slot = dyn-Blatt ohne Quelle.
        List<Id> slots = new ArrayList<>();
        List<Id> created = new ArrayList<>();
        for (int planIx = 0; planIx < r.createNodes.size(); planIx++) {
            CreateNode cn = r.createNodes.get(planIx);
            Id parent;
            if (cn.parent >= 0) {
                parent = refs[cn.parent];
            } else {
                parent = slots.get(-cn.parent - 1);
                if (parent == null) { slots.add(null); continue; }
            }
            Id id;
            if (cn.corrFullMatch) {
                id = g.addCorr(parent, cn.typ, refs);
            } else if (cn.dynAttr != null) {
                Id src = g.childLeafOfType(refs[cn.dynAnchor], cn.dynAttr);
                if (src == null) { slots.add(null); continue; }
                id = g.addDerivedLeaf(parent, cn.typ, src, cn.dynTransform);
            } else if (cn.konst != null) {
                id = g.addKonstLeaf(parent, cn.typ, r.name, planIx, cn.konst);
            } else if (cn.derivedLeaf < 0) {
                id = g.addGhost(parent, cn.typ);
            } else {
                id = g.addDerivedLeaf(parent, cn.typ, refs[cn.derivedLeaf], cn.derivedTransform);
            }
            slots.add(id);
            created.add(id);
        }
        List<Id> edges = new ArrayList<>();
        for (int[] l : r.createLinks) {
            Id s = l[0] >= 0 ? refs[l[0]] : slots.get(-l[0] - 1);
            Id t = l[1] >= 0 ? refs[l[1]] : slots.get(-l[1] - 1);
            if (s != null && t != null) {
                // Nur NEU angelegte Kanten erfassen (fresh) — eine
                // wiederverwendete gehört bereits zum Erzeuger.
                Graph.ConnResult cr = g.connectReporting(s, t, St.GHOST);
                if (cr != null && cr.fresh) edges.add(cr.id);
            }
        }
        return new Created(created, edges);
    }

    /** true = angewandt, false = Duplikat, null = fertig. */
    public Boolean step(Graph g, Map<Id, String> vals) {
        return stepWithLimit(g, vals, null);
    }

    /** Schritt mit Backtracking-Schranke (strikt UNTER der Schranke). */
    public Boolean stepWithLimit(Graph g, Map<Id, String> vals, SelectionBound ceiling) {
        while (true) {
            TodoKey k;
            if (ceiling == null) {
                if (todo.isEmpty()) return null;
                k = todo.first();
            } else {
                TodoKey bound = new TodoKey(ceiling.rank, ceiling.refs, 0);
                k = null;
                for (TodoKey c : todo.tailSet(bound, true)) {
                    if (c.rank == bound.rank && Matcher.compareBindings(c.refs, bound.refs) == 0) continue;
                    k = c;
                    break;
                }
                if (k == null) return null;
            }
            todo.remove(k);
            Rule rule = rules.get(k.ruleIx);
            AppliedKey ak = new AppliedKey(k.ruleIx, k.refs);
            if (applied.contains(ak)) continue;
            // TT-Anker feuern nicht (Reclaim-Zombie-Schutz): vergessener
            // Match wird vom nächsten Seed neu gefunden, falls resurrektiert.
            boolean anchorTt = false;
            for (Object[] rec : rule.corrRecognition) {
                int pos = (Integer) rec[1];
                Node an = g.node(k.refs[pos]);
                if (an != null && an.status == St.TENTATIVE_TOMBSTONE) { anchorTt = true; break; }
            }
            if (anchorTt) { byKey.remove(key(k.ruleIx, k.refs)); continue; }
            boolean translated = !rule.corrRecognition.isEmpty();
            for (Object[] rec : rule.corrRecognition) {
                String typ = (String) rec[0];
                int pos = (Integer) rec[1];
                String endTyp = (String) rec[2];
                boolean found = false;
                Integer t = g.lookup(typ);
                Integer ept = g.lookup(endTyp);
                if (t != null && ept != null) {
                    for (Part p : g.partsByOtherType(k.refs[pos], t)) {
                        Conn c = g.conn(p.connection);
                        Node other = g.node(p.other);
                        if (c == null || !c.status.alive()
                                || other == null || !other.status.alive()) continue;
                        for (Part q : g.parts(p.other)) {
                            if (!q.other.equals(k.refs[pos]) && q.otherTyp == ept) {
                                Node en = g.node(q.other);
                                if (en != null && en.status.alive()) { found = true; break; }
                            }
                        }
                        if (found) break;
                    }
                }
                if (!found) { translated = false; break; }
            }
            if (translated || creationExists(g, rule, k.refs)) {
                applied.add(ak);
                return false;
            }
            // V7-Guard
            boolean contradicts = false;
            List<Id> ids = new ArrayList<>();
            for (CreateNode cn : rule.createNodes) {
                Id id = previewCreate(g, rule, k.refs, ids, cn);
                if (id == null) {
                    ids.add(cn.parent >= 0 ? k.refs[cn.parent] : ids.get(-cn.parent - 1));
                    continue;
                }
                Node n = g.node(id);
                if (n != null && n.status == St.TOMBSTONE) { contradicts = true; break; }
                ids.add(id);
            }
            if (contradicts) { sawContradiction = true; continue; }
            Created cr = applyCreation(g, rule, k.refs);
            int eix = cascade.size();
            cascade.add(new Entry(k.ruleIx, rule.rank, k.refs, cr.nodes, cr.edges));
            cascadeLen++;
            applied.add(ak);
            // Provenienz-Kante materialisieren (für retractFor ohne Scan).
            Integer mix = byKey.get(key(k.ruleIx, k.refs));
            if (mix != null) matches.get(mix).entry = eix;
            expandAt(g, vals, cr.nodes);
            return true;
        }
    }

    public void runToSaturation(Graph g, Map<Id, String> vals) {
        seed(g, vals);
        while (step(g, vals) != null) { /* weiter */ }
    }

    /** Sättigungs-Verdikt (Spiegel von engine::verdict). Public seit
     *  E2: die Java-native Session mappt es auf das state-Feld des
     *  runCascade-Reports. */
    public Termination verdict() {
        if (sawContradiction) return Termination.CONTRADICTION;
        if (applied.isEmpty()) return Termination.CONVERGENCE;
        return Termination.DUPLICATION;
    }

    /** Cascade bis Sättigung mit Schranke (Spiegel von engine::run). */
    public Termination run(Graph g, Map<Id, String> vals, int maxSteps) {
        seed(g, vals);
        for (int i = 0; i < maxSteps; i++) {
            if (step(g, vals) == null) return verdict();
        }
        return Termination.STEP_LIMIT;
    }
}
