package net.sandrakessler.seesaw.engine;

import net.sandrakessler.seesaw.ident.Ident;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
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
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.Rule;

// ── Engine (Spiegel von engine.rs, Minimal-Pfad) ──
public final class Engine {
    public final List<Rule> rules;

    /** Domäne, in der ein vollständiges Host-Delta entstanden ist. */
    public enum DeltaDomain { SOURCE, TARGET }

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
    /**
     * Element → die Kaskaden-Eintraege, die es ERZEUGT haben.
     * Gegenstueck zu {@link #byElement}, das beantwortet, welche
     * Matches ein Element ENTHALTEN.
     *
     * <p>Ohne ihn kommt die Retraktion vom gefallenen Erzeugnis nicht
     * zu dem Eintrag, der es gebaut hat: der Match einer Regel enthaelt
     * nicht, was die Regel erzeugt. Die Korrespondenz desselben
     * Eintrags war damit unerreichbar (gemessen 2026-08-11).
     */
    final HashMap<Id, ArrayList<Integer>> byProduct = new HashMap<>();
    /** Null = absichtlich ungerichteter Initial-/Batchlauf. */
    boolean[] waveDirections;
    /**
     * Elemente, die der HOST geloescht hat, im Unterschied zu solchen,
     * die die Engine selbst zurueckgezogen hat.
     *
     * <p>WELLENLOKAL: gefuellt von {@link #elementDeleted} und von der
     * Transitivitaet, geleert von {@link #consolidate}. Die Markierung
     * gehoert der Welle, nicht der Identitaet -- als dauerhafte Menge
     * waere sie bei Undo oder einem Baseline-Wechsel falsch (Review
     * 2026-08-11, Punkt 3).
     */
    final TreeSet<Id> hostDeleted = new TreeSet<>();
    final ArrayList<Id> pendingTtNodes = new ArrayList<>();
    final ArrayList<Id> pendingTtEdges = new ArrayList<>();

    /**
     * Sucht die Engine ueber den {@link AnchorIndex}? {@code false}: die
     * ungefilterte Suche wie in 2.0.2 (jeder neue Knoten an jeder
     * aktiven Regel an jeder Position seines Typs, ein Scan je Regel im
     * Seed). Beide Wege tragen dieselben Matches in derselben
     * Reihenfolge ein.
     */
    public final boolean useIndex;
    /** Der Index: selbst gebaut beim ersten Gebrauch oder vom Host
     *  uebergeben. */
    private AnchorIndex index;
    /** Groesse der Regelliste, fuer die {@link #index} gilt. */
    private int indexRules = -1;
    private final AnchorIndex.Walk walk = new AnchorIndex.Walk();
    /** Zahl der gestarteten Suchen (Expansion und Seed) und der Seeds,
     *  die ueber den Index gingen. Diagnose fuer Tests und Messung. */
    long searches;
    long seedsByIndex;

    /**
     * Systemeigenschaft {@code seesaw.anchorIndex}: {@code false}
     * schaltet den Index fuer alle Engines ab, die ohne ausdrueckliche
     * Wahl angelegt werden.
     */
    public static boolean defaultUseIndex() {
        return !"false".equalsIgnoreCase(System.getProperty("seesaw.anchorIndex"));
    }

    /**
     * Systemeigenschaft {@code seesaw.checks=true}: jede Expansion und
     * jeder Seed einer Engine mit Index laeuft zusaetzlich ungefiltert,
     * und die beiden Folgen von Matches werden verglichen. Weicht eine
     * ab, wirft die Engine. Gegenstueck zu {@code --cfg seesaw_checks}
     * auf der Rust-Seite; damit laufen alle Tests unter der Pruefung.
     */
    static final boolean CHECKS = Boolean.getBoolean("seesaw.checks");

    public Engine(List<Rule> rules) { this(rules, defaultUseIndex()); }

    /** Mit ausdruecklicher Wahl des Suchwegs, siehe {@link #useIndex}. */
    public Engine(List<Rule> rules, boolean useIndex) {
        this.rules = rules;
        this.useIndex = useIndex;
    }

    /**
     * Mit einem Index, den der Host fuer denselben Regelsatz einmal
     * gebaut hat ({@link AnchorIndex#build}) und den mehrere Engines
     * teilen. Ein Index, der zu anderen Mustern gehoert
     * ({@link AnchorIndex#fits}), wird nicht genommen: die Engine baut
     * dann ihren eigenen.
     */
    public Engine(List<Rule> rules, AnchorIndex shared) {
        this(rules, true);
        if (shared != null && shared.fits(rules)) {
            index = shared;
            indexRules = rules.size();
        }
    }

    /**
     * Der Index der Engine, gebaut, wenn er fehlt oder die Regelliste
     * seither ihre Laenge geaendert hat. {@code null}: die Regeln
     * sprengen die Kodierung des Index ({@link AnchorIndex#supports}),
     * gesucht wird dann ungefiltert. Eine Regel, die in der Liste an
     * ihrem Platz ERSETZT oder veraendert wird, sieht die Pruefung
     * nicht (wie auf der Rust-Seite).
     */
    private AnchorIndex index() {
        if (indexRules != rules.size()) {
            index = AnchorIndex.supports(rules) ? AnchorIndex.build(rules) : null;
            indexRules = rules.size();
        }
        return index;
    }

    /** Abnehmer der gefundenen Matches eines Suchwegs. */
    private interface Sink {
        void match(int ruleIx, Id[] refs);
    }

    /** Die Folge der Matches eines Suchwegs, fuer den Vergleich. */
    private static final class Found implements Sink {
        final ArrayList<Integer> rule = new ArrayList<>();
        final ArrayList<Id[]> refs = new ArrayList<>();

        @Override public void match(int ruleIx, Id[] r) {
            rule.add(ruleIx);
            refs.add(r);
        }

        void into(Sink sink) {
            for (int i = 0; i < rule.size(); i++) sink.match(rule.get(i), refs.get(i));
        }

        /** Stelle der ersten Abweichung, -1 = gleich. */
        int differsAt(Found o) {
            int n = Math.min(rule.size(), o.rule.size());
            for (int i = 0; i < n; i++) {
                if (!rule.get(i).equals(o.rule.get(i))
                        || Matcher.compareBindings(refs.get(i), o.refs.get(i)) != 0) {
                    return i;
                }
            }
            return rule.size() == o.rule.size() ? -1 : n;
        }
    }

    private void checkSame(String what, Found indexed, Found plain) {
        int at = indexed.differsAt(plain);
        if (at >= 0) {
            String rule = at < plain.rule.size() ? rules.get(plain.rule.get(at)).name
                    : rules.get(indexed.rule.get(at)).name;
            throw new IllegalStateException("anchor index: " + what + " weicht von der ungefilterten "
                    + "Suche ab, Stelle " + at + " (mit Index " + indexed.rule.size()
                    + " Matches, ohne " + plain.rule.size() + "), Regel " + rule);
        }
    }

    /** Ein vollständiges externes Delta vor seinen Add-/Change-/Delete-
     *  Tueren zulassen. Die Domänenvereinigung bleibt für die ganze
     *  Realisationswelle unverändert. */
    public void admitDelta(List<DeltaDomain> domains) {
        boolean forward = domains.contains(DeltaDomain.SOURCE);
        boolean backward = domains.contains(DeltaDomain.TARGET);
        waveDirections = new boolean[] { forward, backward };
    }

    /** Nach einer gerichteten Welle wieder Initial-/Batchsemantik. */
    public void admitInitial() {
        waveDirections = null;
    }

    private void admitDeltaTypes(List<String> deltaTypes) {
        boolean forward = false;
        boolean backward = false;
        for (Rule r : rules) {
            boolean hit = false;
            for (String t : r.inputTypes) {
                if (deltaTypes.contains(t)) { hit = true; break; }
            }
            if (!hit) continue;
            if (r.direction == Rule.Direction.FORWARD) forward = true;
            if (r.direction == Rule.Direction.BACKWARD) backward = true;
        }
        waveDirections = new boolean[] { forward, backward };
    }

    private boolean ruleActive(int ri) {
        if (waveDirections == null) return true;
        Rule.Direction d = rules.get(ri).direction;
        return d == Rule.Direction.UNDIRECTED
                || (d == Rule.Direction.FORWARD && waveDirections[0])
                || (d == Rule.Direction.BACKWARD && waveDirections[1]);
    }

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
        // Die KANTEN des Matches gehoeren ebenso in die Registratur, nicht
        // nur seine Knoten. Ein Match ruht auf den Kanten, die sein Muster
        // verlangt, also muss ihr Wegfall ihn ebenso toeten wie der eines
        // Knotens. Bis hierher toetete ein reines Kanten-Delta (Owner- oder
        // Super-Wechsel) keinen einzigen Match. Spiegel von record in
        // seesaw-core/src/engine/mod.rs.
        //
        // SAME_VALUE ist ein Wert-Join ueber zwei Blaetter, keine Kante --
        // im Graphen gibt es dazu keine Verbindung zu indizieren.
        for (PatLink l : rules.get(ruleIx).patLinks) {
            if (l.kind == PatLink.Kind.SAME_VALUE) continue;
            if (l.from < refs.length && l.to < refs.length) {
                Id e = Ident.identConnection(refs[l.from], refs[l.to]);
                byElement.computeIfAbsent(e, x -> new ArrayList<>()).add(ix);
            }
        }
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
            // Zwei Wege, auf denen ein Element weitere Retraktion
            // begruendet.
            //
            // Es NIMMT TEIL an Matches: die verlieren ihren Boden.
            ArrayList<Integer> mixes = byElement.get(id);
            if (mixes != null) {
                for (int mix : new ArrayList<>(mixes)) retractMatch(g, mix, queue);
            }
            // Es wurde von Eintraegen ERZEUGT: die verlieren ihr
            // Ergebnis, und mit ihnen faellt alles Weitere, das sie
            // gebaut haben -- die Korrespondenz vor allem.
            ArrayList<Integer> eixs = byProduct.get(id);
            if (eixs != null) {
                for (int eix : new ArrayList<>(eixs)) retractEntry(g, eix, queue);
            }
        }
    }

    /**
     * Retraktion ueber den EINTRAG: den Match finden, der ihn erzeugt
     * hat, und diesen zurueckziehen. Nutzt {@link #retractMatch}, damit
     * die Reclaim-Buchhaltung (applied/byKey) an einer Stelle bleibt.
     */
    void retractEntry(Graph g, int eix, ArrayList<Id> queue) {
        Entry e = cascade.get(eix);
        Integer mix = byKey.get(key(e.ruleIx, e.refs));
        if (mix != null) retractMatch(g, mix, queue);
    }

    /**
     * <b>Der Host hat ein Element GELOESCHT.</b> Eine der drei Tueren
     * des Interfaces zwischen Host und Engine (add / delete / update).
     *
     * <p>Der Delta-Typ kommt vom AUFRUF, nicht aus einer
     * Statuspruefung. Bis 2026-08-13 gab es nur {@code retractFor}, und
     * die Engine las am Status ab, was gemeint war: tot beim Eintreffen
     * hiess Delete, lebendig hiess Update. Das war eine Absprache
     * zwischen Host und Engine, keine Schnittstelle.
     *
     * <p>Der Aufrufer hat den Tombstone bereits gesetzt. Spiegel von
     * {@code element_deleted}.
     */
    public void elementDeleted(Graph g, Id removed) {
        hostDeleted.add(removed);
        ArrayList<Id> queue = new ArrayList<>();
        queue.add(removed);
        drainRetraction(g, queue);
    }

    /**
     * <b>Der Host hat ein Element GEAENDERT.</b> Die dritte Tuer.
     *
     * <p>Zurueckziehen und neu ableiten, ohne dass etwas entlang der
     * Korrespondenzen faellt: ein Change, der seine Realisierung
     * verletzt, zieht sie samt Korrespondenz zurueck, macht daraus aber
     * kein Delete der Gegenseite.
     */
    public void elementChanged(Graph g, Id changed) {
        ArrayList<Id> queue = new ArrayList<>();
        queue.add(changed);
        drainRetraction(g, queue);
    }

    /**
     * @deprecated {@link #elementDeleted} oder {@link #elementChanged}
     *     benutzen -- der Delta-Typ gehoert zum Aufruf, nicht zu einer
     *     Statuspruefung.
     */
    @Deprecated
    public void retractFor(Graph g, Id removed) {
        // Beim Eintreffen tot = der Host hat geloescht. Lebendig = die
        // Engine bewertet neu (ein Change), und entlang der
        // Korrespondenzen darf nichts fallen.
        Node rn = g.node(removed);
        if (rn == null || !rn.status.matchable()) hostDeleted.add(removed);
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
        ArrayList<Id> fallen = new ArrayList<>();
        for (Id id : pendingTtNodes) {
            Node n = g.node(id);
            if (n != null && n.status == St.TENTATIVE_TOMBSTONE) {
                g.setNodeStatus(id, St.TOMBSTONE);
                if (n.isCorr) fallen.add(id);
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
        int e2 = eliminated + followFallenCorrs(g, fallen);
        // Die Markierung gehoert der Welle, die gerade endete.
        hostDeleted.clear();
        return e2;
    }

    /**
     * <b>Loeschen entlang der Korrespondenzen</b> (Sandra 2026-08-11).
     *
     * <p>Eine Korrespondenz, die die Konsolidierung als Tombstone
     * ueberstanden hat, bezeugt eine Uebersetzung, die nicht mehr gilt.
     * Die Elemente, die sie verbindet, fallen mit ihr. Das traegt eine
     * Loeschung auf die andere Seite: wer die erzeugte Java-Klasse
     * loescht, loescht die UML-Klasse.
     *
     * <p>Warum HIER und nicht in {@link #drainRetraction}: Retraktion
     * ist auch das interne Mittel fuer einen Change (zurueckziehen, neu
     * ableiten, reklamieren). Mitten im Lauf sagt eine gefallene
     * Korrespondenz nichts aus. Erst die Konsolidierung entscheidet.
     *
     * <p>Das Kriterium ist der Delta-Typ, nicht der Status. Ein Change,
     * der seine Realisierung haelt, wirkt entlang derselben
     * Korrespondenz. Ein Change, der sie VERLETZT, zieht sie samt
     * Korrespondenz zurueck -- aber daraus wird kein Delete der
     * Gegenseite. Nur ein Delete traegt weiter, und nur ueber eine
     * Korrespondenz, die die Konsolidierung gefallen liess.
     *
     * <p>Spiegel von {@code follow_fallen_corrs} in
     * {@code seesaw-core/src/engine/mod.rs}.
     */
    int followFallenCorrs(Graph g, ArrayList<Id> fallen) {
        int eliminated = 0;
        TreeSet<Id> seen = new TreeSet<>();
        while (!fallen.isEmpty()) {
            Id corr = fallen.remove(fallen.size() - 1);
            Node cn = g.node(corr);
            if (!seen.add(corr) || cn == null || !cn.isCorr) continue;
            ArrayList<Id> ends = new ArrayList<>();
            for (Part p : g.parts(corr)) ends.add(p.other);
            boolean geloescht = false;
            for (Id e : ends) if (hostDeleted.contains(e)) geloescht = true;
            if (!geloescht) continue;
            for (Id end : ends) {
                Node en = g.node(end);
                if (en == null || !en.status.matchable()) continue;
                g.setNodeStatus(end, St.TOMBSTONE);
                // Transitivitaet: ein entlang einer Korrespondenz
                // mitgerissenes Element ist geloescht und traegt das
                // naechste weiter.
                hostDeleted.add(end);
                elementRemoved(end);
                eliminated++;
                ArrayList<Id> queue = new ArrayList<>();
                queue.add(end);
                drainRetraction(g, queue);
                for (Id id : pendingTtNodes) {
                    Node n = g.node(id);
                    if (n != null && n.status == St.TENTATIVE_TOMBSTONE) {
                        g.setNodeStatus(id, St.TOMBSTONE);
                        if (n.isCorr) fallen.add(id);
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
            }
        }
        return eliminated;
    }

    public void seedRule(Graph g, Map<Id, String> vals, int ri) {
        Rule r = rules.get(ri);
        Id[] fixed = new Id[r.patNodes.size()];
        for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) record(ri, m);
    }

    /**
     * Anfangs-Enumeration: alle zugelassenen Regeln ueber den ganzen
     * Graphen. Die Matches werden Regel fuer Regel eingetragen, die
     * einer Regel in mu-Ordnung. Gefunden werden sie entweder durch
     * einen Scan je Regel oder ueber den {@link AnchorIndex}, je
     * nachdem, was weniger Knoten ansieht; beide Wege finden dieselben
     * Matches und tragen sie in derselben Reihenfolge ein.
     */
    public void seed(Graph g, Map<Id, String> vals) {
        if (useIndex && index() != null) {
            if (CHECKS) {
                Found indexed = new Found();
                Found plain = new Found();
                seedByIndex(g, vals, indexed);
                seedByScan(g, vals, plain);
                checkSame("der Seed", indexed, plain);
                indexed.into(this::record);
                return;
            }
            if (seedThroughIndex(g)) {
                seedByIndex(g, vals, this::record);
                return;
            }
        }
        seedByScan(g, vals, this::record);
    }

    /** Ein Scan je zugelassener Regel ueber den ganzen Graphen. Die
     *  Suche beginnt bei den Knoten vom Typ der ersten Position. */
    private void seedByScan(Graph g, Map<Id, String> vals, Sink sink) {
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            Id[] fixed = new Id[r.patNodes.size()];
            searches++;
            for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) sink.match(ri, m);
        }
    }

    /**
     * Geht der Seed ueber den {@link AnchorIndex}?
     *
     * <p>Der Scan sieht die Knoten vom Typ der ersten Position einmal
     * je Regel an, der Index geht jeden Knoten einmal fuer alle Regeln
     * ab. In einem Regelsatz, dessen Regeln einen Strukturtyp teilen,
     * kostet der Scan Regeln mal Knoten, der Gang Knoten. In einem
     * Regelsatz mit eigenem Typ je Regel sieht der Scan weniger Knoten
     * an, als der Graph hat, und der Gang kostet je Knoten mehr. Der
     * Index wird genommen, wenn der Scan bei mehr als dem Vierfachen
     * der Knoten des Graphen beginnen wuerde (Schwelle wie in Rust).
     */
    private boolean seedThroughIndex(Graph g) {
        long starts = 0;
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            if (!r.patNodes.isEmpty()) starts += g.typeCount(r.patNodes.get(0).typ);
        }
        return starts > seedFactor * g.nodeCount();
    }

    /** Schwelle von {@link #seedThroughIndex}. Tests setzen 0, damit
     *  der Seed ueber den Index geht, sobald ein Scan irgendwo
     *  beginnen wuerde. */
    long seedFactor = 4;

    /**
     * Der Seed ueber den {@link AnchorIndex}.
     *
     * <p>Der Scan einer Regel setzt jeden matchbaren Knoten vom Typ der
     * ersten Position an Position 0 und sucht von dort weiter. Die
     * verankerte Suche mit dem Knoten an Position 0 ist genau dieser
     * Teil des Scans: ihr Plan beginnt mit Position 0 und waehlt die
     * weiteren Positionen wie der Plan des Scans. Der Index nennt die
     * Knoten, bei denen diese Suche einen Match finden kann (seine
     * Bedingung ist notwendig), die uebrigen fielen im Scan leer aus.
     * Die Matches einer Regel werden wie im Scan sortiert, also ist die
     * Folge dieselbe, samt Wiederholungen. Ein Muster ohne Knoten hat
     * keinen Platz im Index und wird direkt gesucht.
     */
    private void seedByIndex(Graph g, Map<Id, String> vals, Sink sink) {
        AnchorIndex ix = index();
        seedsByIndex++;
        // Die Knoten, an denen ein Scan beginnen wuerde: die matchbaren
        // vom Typ der ersten Position einer zugelassenen Regel.
        ArrayList<Id> starts = new ArrayList<>();
        HashSet<Integer> types = new HashSet<>();
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            if (!r.patNodes.isEmpty() && types.add(r.patNodes.get(0).typ)) {
                starts.addAll(g.nodesOfType(r.patNodes.get(0).typ));
            }
        }
        long[] candidates = ix.candidates(g, vals, starts, walk, this::ruleActive, true);
        int next = 0;
        ArrayList<Id[]> found = new ArrayList<>();
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            int n = r.patNodes.size();
            if (n == 0) {
                for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, new Id[0])) sink.match(ri, m);
                continue;
            }
            found.clear();
            List<Matcher.Step> plan = null;
            while (next < candidates.length && AnchorIndex.rule(candidates[next]) == ri) {
                Id[] fixed = new Id[n];
                fixed[0] = starts.get(AnchorIndex.node(candidates[next]));
                next++;
                if (plan == null) plan = ix.plan(ri, 0, r);
                searches++;
                found.addAll(Matcher.findMatchesPlanned(g, vals, r, plan, fixed));
            }
            found.sort(Matcher::compareBindings);
            for (Id[] m : found) sink.match(ri, m);
        }
        if (next != candidates.length) {
            throw new IllegalStateException("anchor index: Kandidat einer nicht zugelassenen Regel");
        }
    }

    /** Kompatibilitätsroute für Hosts mit berührten Typnamen. Die
     *  Typen wählen eine RICHTUNG; alle Regeln dieser Richtung bleiben
     *  für gleichgerichtete Folgerealisierungen aktiv. */
    public void seedRouted(Graph g, Map<Id, String> vals, List<String> deltaTypes) {
        admitDeltaTypes(deltaTypes);
        seed(g, vals);
    }

    /** Add-Strom (Spiegel von elements_added): extern hinzugefügte
     *  Elemente delta-lokal ankern; danach step bis Sättigung. */
    public void elementsAdded(Graph g, Map<Id, String> vals, List<Id> newNodes) {
        expandAt(g, vals, newNodes);
    }

    /**
     * Ein Link ist HINZUGEKOMMEN (Gegenstueck zu
     * {@link #linkRemoved}). Spiegel von {@code link_added}.
     *
     * <p>Ein {@code connect} auf zwei BESTEHENDE Knoten weckt nichts:
     * die Anker-Expansion laeuft ueber neue KNOTEN, und beide Enden
     * waren schon da. Ein reines Add-Kanten-Delta (Owner-Wechsel,
     * Generalisierung) bliebe damit unsichtbar.
     *
     * <p>Beide Enden neu zu expandieren ist dieselbe
     * Ueber-Approximation wie bei {@code linkRemoved}. Die
     * Duplikat-Unterdrueckung faengt ueberfluessige Kandidaten ab, weil
     * Identitaeten ableitbar sind.
     */
    public void linkAdded(Graph g, Map<Id, String> vals, Id a, Id b) {
        expandAt(g, vals, List.of(a, b));
    }

    /**
     * Delta-lokale Expansion: neue Elemente verankern (Spec §1.6 add).
     *
     * <p>Mit Index nur an den (Regel, Knoten, Position)-Tripeln, deren
     * Nachbarschaft die Pfade verwirklicht, die das Muster von dort
     * verlangt ({@link AnchorIndex}). Die Tripel kommen in der
     * Reihenfolge der Schleife ueber Regeln, Knoten und Positionen, und
     * an jedem laeuft dieselbe Suche wie ungefiltert: eingetragen wird
     * also dieselbe Folge von Matches, nur die Suchen ohne Ergebnis
     * entfallen.
     *
     * <p>Jeder gefundene Match wird eingetragen, auch einer, dessen
     * Anker schon uebersetzt ist: das entscheidet {@link #step}, wenn
     * er den Match zieht, auf dem Graphen, wie er dann ist.
     */
    public void expandAt(Graph g, Map<Id, String> vals, List<Id> newNodes) {
        if (useIndex && index() != null) {
            if (CHECKS) {
                Found indexed = new Found();
                Found plain = new Found();
                expandByIndex(g, vals, newNodes, indexed);
                expandUnfiltered(g, vals, newNodes, plain);
                checkSame("die Expansion", indexed, plain);
                indexed.into(this::record);
                return;
            }
            expandByIndex(g, vals, newNodes, this::record);
            return;
        }
        expandUnfiltered(g, vals, newNodes, this::record);
    }

    /** Die Expansion von 2.0.2: jeder neue Knoten an jeder aktiven
     *  Regel an jeder Position seines Typs. */
    private void expandUnfiltered(Graph g, Map<Id, String> vals, List<Id> newNodes, Sink sink) {
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            for (Id id : newNodes) {
                Node n = g.node(id);
                if (n == null) continue;
                for (int pos = 0; pos < r.patNodes.size(); pos++) {
                    if (r.patNodes.get(pos).typ != n.typ) continue;
                    Id[] fixed = new Id[r.patNodes.size()];
                    fixed[pos] = id;
                    searches++;
                    for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) sink.match(ri, m);
                }
            }
        }
    }

    private void expandByIndex(Graph g, Map<Id, String> vals, List<Id> newNodes, Sink sink) {
        if (newNodes.isEmpty()) return;
        AnchorIndex ix = index();
        // Die Suchen lesen nur den Graphen, das Eintragen aendert ihn
        // nicht: die Kandidaten gelten fuer die ganze Expansion.
        long[] candidates = ix.candidates(g, vals, newNodes, walk, this::ruleActive, false);
        for (long t : candidates) {
            int ri = AnchorIndex.rule(t);
            int pos = AnchorIndex.pos(t);
            Rule r = rules.get(ri);
            Id[] fixed = new Id[r.patNodes.size()];
            fixed[pos] = newNodes.get(AnchorIndex.node(t));
            searches++;
            for (Id[] m : Matcher.findMatchesPlanned(g, vals, r, ix.plan(ri, pos, r), fixed)) {
                sink.match(ri, m);
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
        List<Id[]> dynCorrs = new ArrayList<>();
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
                // Die Attribut-Korrespondenz des dynamischen Falls.
                // Fehlt die Quelle, entsteht kein Blatt und folglich
                // auch keine Korrespondenz -- apply-if-present gilt
                // fuer beide. Spiegel von apply_creation in plan.rs.
                if (cn.attrCorr != null) {
                    Id ac = g.addCorr(src, cn.attrCorr + "_" + cn.typ, refs);
                    g.markCorr(ac);
                    dynCorrs.add(new Id[] {ac, src, id});
                }
            } else if (cn.konst != null) {
                id = g.addKonstLeaf(parent, cn.typ, r.name, planIx, cn.konst);
            } else if (cn.derivedLeaf < 0) {
                id = g.addGhost(parent, cn.typ);
            } else {
                id = g.addDerivedLeaf(parent, cn.typ, refs[cn.derivedLeaf], cn.derivedTransform);
            }
            if (cn.isCorr) g.markCorr(id);
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
        // Attribut-Korrespondenzen des dynamischen Falls: erst hier
        // bekannt, weil ihre Quelle beim Anwenden gefunden wurde.
        for (Id[] dc : dynCorrs) {
            created.add(dc[0]);
            for (int k = 1; k <= 2; k++) {
                Id e = g.connect(dc[0], dc[k], St.GHOST);
                if (e != null) edges.add(e);
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
            TodoKey k = null;
            if (ceiling == null) {
                if (waveDirections == null) {
                    if (!todo.isEmpty()) k = todo.first();
                } else {
                    for (TodoKey c : todo) {
                        if (ruleActive(c.ruleIx)) { k = c; break; }
                    }
                }
            } else {
                TodoKey bound = new TodoKey(ceiling.rank, ceiling.refs, 0);
                for (TodoKey c : todo.tailSet(bound, true)) {
                    if (c.rank == bound.rank && Matcher.compareBindings(c.refs, bound.refs) == 0) continue;
                    if (waveDirections != null && !ruleActive(c.ruleIx)) continue;
                    k = c;
                    break;
                }
            }
            if (k == null) {
                return null;
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
                    for (Part p : g.partsOfType(k.refs[pos], t)) {
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
            // Provenienz, Produktseite: jeder Knoten und jede Kante,
            // die dieser Eintrag gebaut hat, zeigt auf ihn zurueck.
            for (Id pid : cr.nodes) byProduct.computeIfAbsent(pid, x -> new ArrayList<>()).add(eix);
            for (Id pid : cr.edges) byProduct.computeIfAbsent(pid, x -> new ArrayList<>()).add(eix);
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
