package net.sandrakessler.seesaw.engine;

import net.sandrakessler.seesaw.ident.Ident;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HashSet;
import java.util.HexFormat;
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

    /** Domäne, in der ein vollständiges Host-Delta entstanden ist. */
    public enum DeltaDomain { SOURCE, TARGET }

    /**
     * Todo-Schluessel: (Rang absteigend, Ref-Folge absteigend,
     * Regelindex aufsteigend), wie in 2.0.2.
     *
     * <p>Vor der Ref-Folge steht ihr Kopf ({@link #head0},
     * {@link #head1}, Rust {@code head}). Wo sich zwei Koepfe
     * unterscheiden, ordnen sie wie die Folgen selbst, an der
     * Reihenfolge aendert sich also nichts. Der Vergleich muss dann nicht
     * ueber das Array zur ersten Id gehen.
     */
    static final class TodoKey implements Comparable<TodoKey> {
        final long rank;
        /** Die ersten 16 Byte der ersten Id der Folge als zwei long,
         *  Big-Endian; 0 fuer eine leere Folge. */
        final long head0, head1;
        final Id[] refs;
        final int ruleIx;
        /** Schluessel des Matches, derselbe wie im Match-Speicher und in
         *  {@link #byKey}. null in einer Schranke von
         *  {@link #stepWithLimit}. */
        final MatchKey key;
        /** Nummer des Matches im Match-Speicher (Rust {@code Carried}),
         *  nimmt nicht an der Ordnung teil. -1 in einer Schranke. */
        final int mix;

        /** Eine Schranke, ohne Match. */
        TodoKey(long rank, Id[] refs, int ruleIx) {
            this(rank, refs, ruleIx, null, -1);
        }

        /** Der Eintrag des Matches {@code mix}. */
        TodoKey(long rank, MatchKey key, int mix) {
            this(rank, key.refs, key.ruleIx, key, mix);
        }

        private TodoKey(long rank, Id[] refs, int ruleIx, MatchKey key, int mix) {
            this.rank = rank;
            this.head0 = refs.length == 0 ? 0 : word(refs[0].b, 0);
            this.head1 = refs.length == 0 ? 0 : word(refs[0].b, 8);
            this.refs = refs;
            this.ruleIx = ruleIx;
            this.key = key;
            this.mix = mix;
        }

        /** Acht Byte ab {@code from}, Big-Endian. */
        static long word(byte[] b, int from) {
            long w = 0;
            for (int i = from; i < from + 8; i++) w = (w << 8) | (b[i] & 0xFF);
            return w;
        }

        @Override public int compareTo(TodoKey o) {
            // (Reverse(rank), Reverse(head), Reverse(refs), ruleIx). Die
            // Koepfe ohne Vorzeichen, wie die Byte-Ordnung der Id.
            int c = Long.compare(o.rank, rank);
            if (c != 0) return c;
            c = Long.compareUnsigned(o.head0, head0);
            if (c != 0) return c;
            c = Long.compareUnsigned(o.head1, head1);
            if (c != 0) return c;
            c = Matcher.compareBindings(o.refs, refs);
            if (c != 0) return c;
            return Integer.compare(ruleIx, o.ruleIx);
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
        /** Regelindex und Ref-Folge als Schluessel, dasselbe Objekt im
         *  Todo und in {@link #byKey}. */
        final MatchKey key;
        boolean dead;
        /** Provenienz-Kante Match → Kaskaden-Eintrag (null = nicht angewandt). */
        Integer entry;

        MatchRec(MatchKey key) { this.ruleIx = key.ruleIx; this.refs = key.refs; this.key = key; }
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
    /** Die angewandten Matches (Rust {@code applied}): eine Marke je
     *  Nummer eines Matches, siehe {@link Applied}. */
    final Applied applied;
    /** (Regelindex, Ref-Folge) → Index im Match-Speicher. */
    final HashMap<MatchKey, Integer> byKey = new HashMap<>();
    final ArrayList<MatchRec> matches = new ArrayList<>();
    /** Knoten → Indizes der Matches, die ihn binden, in der Reihenfolge des
     *  Eintragens. Die Matches, die auf einer Verbindung ruhen, stehen nicht
     *  hier, {@link Links#filedUnder} findet sie aus den Listen ihrer Enden. */
    final HashMap<Id, IntList> byElement = new HashMap<>();
    /** Was die Engine ueber die Links ihrer Matches neben
     *  {@link #byElement} fuehrt. */
    final Links links = new Links(this);
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
     *
     * <p>Abgelegt werden die erzeugten KNOTEN. Die Eintraege, die eine
     * erzeugte Verbindung gebaut haben, findet {@link #producersOf} aus
     * ihren beiden Enden, wenn eine geloeschte Verbindung danach fragt.
     * Jede Verbindung abzulegen war mehr als die Haelfte der Eintraege
     * (Rust aff0c2c).
     */
    final HashMap<Id, IntList> byProduct = new HashMap<>();
    /**
     * Pruefung von {@link #producersOf} unter {@link #CHECKS}: die
     * erzeugten Verbindungen, abgelegt wie bisher in {@link #byProduct}
     * (Rust {@code by_product_filed}). null ohne Pruefung.
     */
    final HashMap<Id, IntList> byProductFiled = CHECKS ? new HashMap<>() : null;
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
    /** Arbeitsgedaechtnis der Gaenge durch den Index, mit dem Zaehler
     *  der Gaenge ({@link AnchorIndex.Walk#visits}). */
    final AnchorIndex.Walk walk = new AnchorIndex.Walk();
    /** Arbeitsspeicher der verankerten Suchen, über die Suchen behalten. */
    private final Matcher.Scratch scratch = new Matcher.Scratch();
    /** Arbeitsspeicher eines Schritts: die Identitaeten der Knoten
     *  seines Plans (Rust {@code ids}), siehe {@link #planOf}. */
    private final ArrayList<Id> ids = new ArrayList<>();
    /** Zahl der gestarteten Suchen (Expansion und Seed) und der Seeds,
     *  die ueber den Index gingen. Diagnose fuer Tests und Messung. */
    long searches;
    long seedsByIndex;
    /**
     * Zaehler an den Stellen der Rust-Zaehler aus {@code perf_trace}:
     * die verankerten Suchen ueber den Index in der Expansion und im
     * Seed ({@code anchored_searches}) und die Suchen je Regel im Seed
     * ohne Index ({@code seed_searches}). Diagnose fuer Tests.
     */
    long anchoredSearches;
    long seedSearches;

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
     * Ebenso die Pruefungen von {@link #producersOf} und des Schritts
     * ({@link #checks}).
     */
    static final boolean CHECKS = Boolean.getBoolean("seesaw.checks");

    /**
     * Prueft der Schritt, was er ueber die Nummer des gezogenen Matches
     * weiss? Dann fuehrt {@link Applied} die Menge der angewandten
     * Schluessel mit, der Match wird zusaetzlich ueber den Schluessel
     * nachgeschlagen ({@link #pulledMatch}), und {@link #recognise}
     * stellt die Anker-Fragen auch einzeln. Eine Abweichung wirft.
     * {@link #CHECKS}, im Test auch fuer eine Engine allein gewaehlt.
     */
    private final boolean checks;

    public Engine(List<Rule> rules) { this(rules, defaultUseIndex()); }

    /** Mit ausdruecklicher Wahl des Suchwegs, siehe {@link #useIndex}. */
    public Engine(List<Rule> rules, boolean useIndex) {
        this(rules, useIndex, CHECKS);
    }

    /** Mit ausdruecklicher Wahl des Suchwegs und der Pruefung des
     *  Schritts, siehe {@link #checks}. */
    Engine(List<Rule> rules, boolean useIndex, boolean checks) {
        this.rules = rules;
        this.useIndex = useIndex;
        this.checks = checks;
        this.applied = new Applied(checks);
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

    /** Bis zu so vielen neuen Knoten wird "steht frueher unter den
     *  neuen Knoten" durch Absuchen der Liste beantwortet, darueber
     *  ueber eine Abbildung (Rust {@code LINEAR_NEW}). */
    static final int LINEAR_NEW = 8;

    /**
     * Steht ein Knoten unter den neuen Knoten einer Expansion vor dem
     * Index {@code ni}? Die Suche und der Gang an {@code newNodes[ni]}
     * lassen solche Knoten aus ({@link #expandByIndex}).
     */
    private static final class Earlier implements AnchorIndex.Skip {
        private final List<Id> nodes;
        /** Je Knoten sein erster Index, ab {@link #LINEAR_NEW} Knoten,
         *  sonst null. */
        private final HashMap<Id, Integer> first;

        Earlier(List<Id> nodes) {
            this.nodes = nodes;
            if (nodes.size() > LINEAR_NEW) {
                first = new HashMap<>();
                for (int i = 0; i < nodes.size(); i++) first.putIfAbsent(nodes.get(i), i);
            } else {
                first = null;
            }
        }

        @Override public boolean test(int ni, Id id) {
            if (first != null) {
                Integer i = first.get(id);
                return i != null && i < ni;
            }
            for (int i = 0; i < ni; i++) {
                if (java.util.Objects.equals(nodes.get(i), id)) return true;
            }
            return false;
        }
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

        /** Die Folge ohne jeden Match, der in ihr schon frueher steht:
         *  die Folge, die {@link Engine#record} davon eintraegt. */
        Found firsts() {
            Found out = new Found();
            HashSet<MatchKey> seen = new HashSet<>();
            for (int i = 0; i < rule.size(); i++) {
                if (seen.add(new MatchKey(rule.get(i), refs.get(i)))) {
                    out.match(rule.get(i), refs.get(i));
                }
            }
            return out;
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
     * Schluessel eines Matches als Zeichenkette: Regelindex plus
     * Referenzfolge. Fuer Hosts; die Engine selbst fuehrt
     * {@link MatchKey}, mit derselben Gleichheit.
     *
     * <p>Feste Breite je Byte. {@code Integer.toHexString} laesst
     * fuehrende Nullen weg, damit war die Kodierung nicht injektiv:
     * die Folgen {@code 01 23} und {@code 12 03} ergaben beide
     * {@code "123"}, und {@link #record}, das damals diese
     * Zeichenkette als Schluessel fuehrte, konnte einen berechtigten
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
        record(null, ruleIx, refs);
    }

    /**
     * Wie {@link #record(int, Id[])}. {@code g} ist der Graph, auf dem der
     * Match gefunden wurde (null von {@link #record(int, Id[])}).
     *
     * <p>Ein Match ruht auch auf den Verbindungen, die sein Muster verlangt,
     * ihr Wegfall toetet ihn ebenso wie der eines Knotens. Abgelegt wird er
     * nur unter seinen Knoten. Die Matches einer Verbindung findet
     * {@link Links#filedUnder} aus deren Enden (Rust 87b270a), fuer eine Id,
     * die der Graph nicht kennt, aus dem Index der nicht gehaltenen Links,
     * den {@link Links#fileLinks} hier weiterfuehrt.
     */
    void record(Graph g, int ruleIx, Id[] refs) {
        MatchKey k = new MatchKey(ruleIx, refs);
        int ix = matches.size();
        // Ein Nachschlagen in byKey: steht der Schluessel schon, bleibt
        // alles, wie es ist. Ein Schluessel je Match, geteilt von
        // Match-Speicher, Todo und byKey.
        if (byKey.putIfAbsent(k, ix) != null) return;
        for (Id id : refs) byElement.computeIfAbsent(id, x -> new IntList()).add(ix);
        Rule rule = rules.get(ruleIx);
        links.fileLinks(g, rule, refs, ix);
        matches.add(new MatchRec(k));
        todo.add(new TodoKey(rule.rank, k, ix));
    }

    /**
     * delete/modify (Spec §1.6): Matches des Elements EAGER töten.
     *
     * <p>Ein Knoten hat seine Liste in {@link #byElement}. Findet sich keine,
     * kann {@code id} eine Verbindung sein, deren Enden nur der Graph kennt,
     * den diese Tuer nicht bekommt. Die Id wird vorgemerkt, die naechste Tuer
     * mit Graph toetet die Matches, die auf ihr ruhen, zuerst
     * ({@link Links#settleRemoved}).
     */
    public void elementRemoved(Id id) {
        IntList refs = byElement.get(id);
        if (refs == null) {
            links.removedUnfiled(id);
            return;
        }
        kill(refs, matches.size());
    }

    /** Toetet die Matches {@code mixes} mit einer Nummer unter
     *  {@code below} sofort: tot und aus dem Todo (Rust {@code kill}). */
    void kill(IntList mixes, int below) {
        for (int i = 0, n = mixes.size(); i < n; i++) {
            int mix = mixes.get(i);
            if (mix >= below) continue;
            MatchRec m = matches.get(mix);
            if (!m.dead) {
                m.dead = true;
                todo.remove(new TodoKey(rules.get(m.ruleIx).rank, m.key, mix));
            }
        }
    }

    /** Ein Match verliert seine Begründung (Kern von retractFor/linkRemoved):
     *  entkräften (dead + To-do), Vermerk vergessen (applied/byKey → Reclaim),
     *  und — falls angewandt — den Erzeugnissen über die Provenienz-Kante
     *  folgen (TT + Queue). Spiegel von retract_match. */
    public void retractMatch(Graph g, int mix, ArrayList<Id> queue) {
        links.settleRemoved(g);
        MatchRec m = matches.get(mix);
        Integer entry = m.entry;
        if (!m.dead) {
            m.dead = true;
            todo.remove(new TodoKey(rules.get(m.ruleIx).rank, m.key, mix));
        }
        // Reclaim-Fähigkeit: Vermerk VERGESSEN, damit eine identische
        // Re-Ableitung erneut anwenden (resurrektieren) kann. Die Marke
        // traegt der Match, der unter dem Schluessel stand.
        applied.forget(m.key, byKey.remove(m.key));
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
                if (g.retractNode(c)) pendingTtNodes.add(c);
                queue.add(c);
            }
            for (Id ed : e.createdEdges) {
                // Dieselbe Begruendung wie oben fuer die Knoten.
                if (g.retractConnection(ed)) pendingTtEdges.add(ed);
            }
        }
    }

    /** Provenienz-Walk über die Queue (Spiegel von drain_retraction). */
    public void drainRetraction(Graph g, ArrayList<Id> queue) {
        links.settleRemoved(g);
        // Nur befuellt und befragt, nie durchlaufen: eine Hash-Menge.
        HashSet<Id> seen = new HashSet<>();
        while (!queue.isEmpty()) {
            Id id = queue.remove(queue.size() - 1);
            if (!seen.add(id)) continue;
            // Zwei Wege, auf denen ein Element weitere Retraktion
            // begruendet.
            //
            // Es NIMMT TEIL an Matches: die verlieren ihren Boden. Ein
            // Knoten bindet sie, auf einer Verbindung ruhen sie.
            // Ohne Kopie der Liste: retractMatch traegt keinen Match ein,
            // aendert also weder byElement noch den Index der nicht
            // gehaltenen Links, und eine IntList waechst nur am Ende. Die
            // Laenge wird vor der Schleife gelesen, die Schleife sieht also
            // genau die Eintraege, die eine Kopie gesehen haette.
            IntList mixes = links.filedUnder(g, id);
            if (mixes != null) {
                for (int i = 0, n = mixes.size(); i < n; i++) retractMatch(g, mixes.get(i), queue);
            }
            // Es wurde von Eintraegen ERZEUGT: die verlieren ihr
            // Ergebnis, und mit ihnen faellt alles Weitere, das sie
            // gebaut haben -- die Korrespondenz vor allem.
            // Ohne Kopie: retractEntry aendert byProduct nicht.
            IntList eixs = producersOf(g, id);
            if (eixs != null) {
                for (int i = 0, n = eixs.size(); i < n; i++) retractEntry(g, eixs.get(i), queue);
            }
        }
    }

    /**
     * Die Kaskaden-Eintraege, die das Element {@code id} erzeugt haben,
     * aufsteigend (Rust {@code producers_of}). null: keiner.
     *
     * <p>Ein Knoten steht in {@link #byProduct}, zurueck kommt dessen
     * Liste selbst (der Aufrufer aendert sie nicht). Eine Verbindung
     * nicht: ihr Erzeuger nennt sie unter seinen {@code createdEdges},
     * und er hat eines ihrer Enden erzeugt oder gematcht (ein Link eines
     * Plans verbindet gematchte und erzeugte Knoten, die Verbindungen
     * einer dynamischen Attribut-Korrespondenz gehen von der
     * Korrespondenz aus, die der Eintrag erzeugt hat). Die Erzeuger der
     * Enden und die Eintraege der Matches, die ein Ende binden, sind
     * damit alle Kandidaten.
     */
    IntList producersOf(Graph g, Id id) {
        Conn c = g.conn(id);
        if (c == null) {
            IntList nodes = byProduct.get(id);
            if (byProductFiled != null) checkProducers(id, false, nodes);
            return nodes;
        }
        // Die Kandidaten: je Ende seine Erzeuger und die Eintraege der
        // Matches, die es binden.
        IntList[] producedBy = {byProduct.get(c.source), byProduct.get(c.target)};
        IntList[] boundIn = {byElement.get(c.source), byElement.get(c.target)};
        int n = 0;
        for (int end = 0; end < 2; end++) {
            if (producedBy[end] != null) n += producedBy[end].size();
            if (boundIn[end] != null) n += boundIn[end].size();
        }
        int[] eixs = new int[n];
        int k = 0;
        for (int end = 0; end < 2; end++) {
            IntList p = producedBy[end];
            if (p != null) {
                for (int i = 0, m = p.size(); i < m; i++) eixs[k++] = p.get(i);
            }
            IntList b = boundIn[end];
            if (b != null) {
                for (int i = 0, m = b.size(); i < m; i++) {
                    Integer entry = matches.get(b.get(i)).entry;
                    if (entry != null) eixs[k++] = entry;
                }
            }
        }
        Arrays.sort(eixs, 0, k);
        IntList producers = null;
        for (int i = 0; i < k; i++) {
            int eix = eixs[i];
            if (i > 0 && eix == eixs[i - 1]) continue;
            if (cascade.get(eix).createdEdges.contains(id)) {
                if (producers == null) producers = new IntList();
                producers.add(eix);
            }
        }
        if (byProductFiled != null) checkProducers(id, true, producers);
        return producers;
    }

    /**
     * Pruefung unter {@link #CHECKS}: {@code found} sind die Erzeuger,
     * die 2.0.2 in {@link #byProduct} abgelegt haette, wo es Knoten und
     * Verbindungen ablegte. Fuer eine Id, die keine Verbindung des
     * Graphen ist, darf unter ihr keine Verbindung abgelegt sein. Fuer
     * eine Verbindung ihre Ablage ohne aufeinanderfolgende
     * Wiederholungen: ein Eintrag, der sie zweimal nennt, stand zweimal
     * in der Liste, der zweite {@link #retractEntry} fand den Schluessel
     * nicht mehr in {@link #byKey} und tat nichts.
     */
    private void checkProducers(Id id, boolean connection, IntList found) {
        IntList filed = byProductFiled.get(id);
        if (!connection) {
            if (filed != null) {
                throw new IllegalStateException("producersOf: unter " + HexFormat.of().formatHex(id.b)
                        + " sind Verbindungen abgelegt " + filed + ", der Graph haelt keine");
            }
            return;
        }
        int[] want = filed == null ? new int[0] : filed.toArray();
        int m = 0;
        for (int i = 0; i < want.length; i++) {
            if (m == 0 || want[m - 1] != want[i]) want[m++] = want[i];
        }
        want = Arrays.copyOf(want, m);
        int[] got = found == null ? new int[0] : found.toArray();
        if (!Arrays.equals(want, got)) {
            throw new IllegalStateException("producersOf: die Erzeuger aus den Enden "
                    + Arrays.toString(got) + " weichen von den abgelegten " + Arrays.toString(want)
                    + " ab, Verbindung " + HexFormat.of().formatHex(id.b));
        }
    }

    /**
     * Retraktion ueber den EINTRAG: den Match finden, der ihn erzeugt
     * hat, und diesen zurueckziehen. Nutzt {@link #retractMatch}, damit
     * die Reclaim-Buchhaltung (applied/byKey) an einer Stelle bleibt.
     */
    void retractEntry(Graph g, int eix, ArrayList<Id> queue) {
        Entry e = cascade.get(eix);
        Integer mix = byKey.get(new MatchKey(e.ruleIx, e.refs));
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
        links.settleRemoved(g);
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
        links.settleRemoved(g);
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
        links.settleRemoved(g);
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
     *  BEIDE Endpunkte enthalten, verlieren ihre Begründung. {@code a} und
     *  {@code b} sind die Enden eines Links, also Knoten, und stehen in
     *  {@link #byElement} mit allem, was 2.0.2 dort unter ihnen ablegte
     *  (unter Pruefung nachgesehen). */
    public void linkRemoved(Graph g, Id a, Id b) {
        links.settleRemoved(g);
        links.checkNotFiled("linkRemoved", a);
        links.checkNotFiled("linkRemoved", b);
        IntList inA = byElement.get(a);
        if (inA == null) return;
        IntList inB = byElement.get(b);
        if (inB == null) return;
        int[] setB = inB.toArray();
        Arrays.sort(setB);
        ArrayList<Id> queue = new ArrayList<>();
        for (int i = 0, n = inA.size(); i < n; i++) {
            int mix = inA.get(i);
            if (Arrays.binarySearch(setB, mix) >= 0) retractMatch(g, mix, queue);
        }
        drainRetraction(g, queue);
    }

    /** Konsolidierung (M5.5): gesammelte TT → Tombstone (O(Δ), inkl.
     *  Kanten). Rückgabe (additiv seit E3, Rust gibt () zurück):
     *  Anzahl finalisierter Elemente = Eliminations-Zählung für den
     *  fold-Report. */
    public int consolidate(Graph g) {
        links.settleRemoved(g);
        int eliminated = 0;
        ArrayList<Id> fallen = new ArrayList<>();
        for (Id id : pendingTtNodes) {
            Node n = g.settleNode(id);
            if (n != null) {
                if (n.isCorr) fallen.add(id);
                eliminated++;
            }
        }
        for (Id id : pendingTtEdges) {
            if (g.settleConnection(id)) eliminated++;
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
        // Nur befuellt und befragt, nie durchlaufen: eine Hash-Menge.
        HashSet<Id> seen = new HashSet<>();
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
                    Node n = g.settleNode(id);
                    if (n != null) {
                        if (n.isCorr) fallen.add(id);
                        eliminated++;
                    }
                }
                for (Id id : pendingTtEdges) {
                    if (g.settleConnection(id)) eliminated++;
                }
                pendingTtNodes.clear();
                pendingTtEdges.clear();
            }
        }
        return eliminated;
    }

    public void seedRule(Graph g, Map<Id, String> vals, int ri) {
        links.settleRemoved(g);
        Rule r = rules.get(ri);
        Id[] fixed = new Id[r.patNodes.size()];
        for (Id[] m : Matcher.findMatchesWithFixed(g, vals, r, fixed)) record(g, ri, m);
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
        links.settleRemoved(g);
        Sink sink = (ri, m) -> record(g, ri, m);
        if (useIndex && index() != null) {
            if (CHECKS) {
                Found indexed = new Found();
                Found plain = new Found();
                seedByIndex(g, vals, indexed);
                seedByScan(g, vals, plain);
                checkSame("der Seed", indexed, plain);
                indexed.into(sink);
                return;
            }
            if (seedThroughIndex(g)) {
                seedByIndex(g, vals, sink);
                return;
            }
        }
        seedByScan(g, vals, sink);
    }

    /** Ein Scan je zugelassener Regel ueber den ganzen Graphen. Die
     *  Suche beginnt bei den Knoten vom Typ der ersten Position. */
    private void seedByScan(Graph g, Map<Id, String> vals, Sink sink) {
        for (int ri = 0; ri < rules.size(); ri++) {
            if (!ruleActive(ri)) continue;
            Rule r = rules.get(ri);
            Id[] fixed = new Id[r.patNodes.size()];
            searches++;
            seedSearches++;
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
        long[] candidates = ix.candidates(g, vals, starts, walk, this::ruleActive, true, null);
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
            Matcher.Plan plan = null;
            while (next < candidates.length && AnchorIndex.rule(candidates[next]) == ri) {
                Id start = starts.get(AnchorIndex.node(candidates[next]));
                next++;
                if (plan == null) plan = ix.plan(ri, 0, r);
                searches++;
                anchoredSearches++;
                Matcher.findMatchesPlanned(g, vals, r, plan, 0, start, scratch);
                found.addAll(scratch.found);
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
     * an jedem laeuft die Suche ohne die frueheren neuen Knoten
     * ({@link #expandByIndex}): eingetragen wird also dieselbe Folge von
     * Matches, nur die Suchen ohne Ergebnis und die Funde, die
     * {@link #record} als schon eingetragen verwerfen wuerde, entfallen.
     *
     * <p>Jeder gefundene Match wird eingetragen, auch einer, dessen
     * Anker schon uebersetzt ist: das entscheidet {@link #step}, wenn
     * er den Match zieht, auf dem Graphen, wie er dann ist.
     */
    public void expandAt(Graph g, Map<Id, String> vals, List<Id> newNodes) {
        links.settleRemoved(g);
        Sink sink = (ri, m) -> record(g, ri, m);
        if (useIndex && index() != null) {
            if (CHECKS) {
                // Der Index sucht einen Match, der mehrere neue Knoten
                // bindet, nur vom ersten aus, die ungefilterte Schleife
                // von jedem. Verglichen wird die Folge der ersten Funde
                // je Match, die Folge, die record eintraegt.
                Found indexed = new Found();
                Found plain = new Found();
                expandByIndex(g, vals, newNodes, indexed);
                expandUnfiltered(g, vals, newNodes, plain);
                checkSame("die Expansion", indexed.firsts(), plain.firsts());
                indexed.into(sink);
                return;
            }
            expandByIndex(g, vals, newNodes, sink);
            return;
        }
        expandUnfiltered(g, vals, newNodes, sink);
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

    /**
     * Die Expansion ueber den {@link AnchorIndex}.
     *
     * <p>Ein Match, der mehrere der neuen Knoten bindet, wuerde von
     * jedem von ihnen aus gefunden. Er wird nur vom ersten aus gesucht:
     * die Suche an {@code newNodes[ni]} laesst jeden Knoten aus, der in
     * {@code newNodes} frueher steht (Rust 878b85f). Ein Match mit einem
     * solchen Knoten hat ihn an einer Position, an der der Index ihn
     * ebenfalls anbietet (die Bedingung ist notwendig), und dieses Tripel
     * sortiert vor diesem. Der Match wird also dort eingetragen, an der
     * Stelle der Folge, an der er bisher zuerst eingetragen wurde, und
     * {@link #record} haette die spaeteren Funde verworfen. Der Index
     * laesst dieselben Knoten aus seinen Gaengen aus (Rust 17b7e52), er
     * bietet also keinen Platz an, dessen Suche nur solche Matches
     * faende, und keinen fuer einen Knoten, der in {@code newNodes} ein
     * zweites Mal steht.
     */
    private void expandByIndex(Graph g, Map<Id, String> vals, List<Id> newNodes, Sink sink) {
        if (newNodes.isEmpty()) return;
        AnchorIndex ix = index();
        Earlier earlier = new Earlier(newNodes);
        // Die Suchen lesen nur den Graphen, das Eintragen aendert ihn
        // nicht: die Kandidaten gelten fuer die ganze Expansion.
        long[] candidates = ix.candidates(g, vals, newNodes, walk, this::ruleActive, false, earlier);
        for (long t : candidates) {
            int ri = AnchorIndex.rule(t);
            int pos = AnchorIndex.pos(t);
            int ni = AnchorIndex.node(t);
            Rule r = rules.get(ri);
            searches++;
            anchoredSearches++;
            // Die Suche schreibt in den Arbeitsspeicher der Engine, der
            // Abnehmer liest die Matches daraus (er sucht nicht selbst).
            Matcher.findMatchesPlanned(g, vals, r, ix.plan(ri, pos, r), pos, newNodes.get(ni),
                    ni == 0 ? null : id -> earlier.test(ni, id), scratch);
            ArrayList<Id[]> found = scratch.found;
            for (int i = 0, k = found.size(); i < k; i++) sink.match(ri, found.get(i));
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

    /** Was der Graph vom Erzeugungsplan einer Regel fuer einen Match
     *  haelt (Rust {@code Planned}). */
    private enum Planned {
        /** Jeder Knoten und jeder Link des Plans ist da, lebendig: die
         *  Regel wuerde nichts erzeugen. */
        EXISTS,
        /** Ein Knoten des Plans ist ein Tombstone. */
        TOMBSTONE,
        /** Etwas vom Plan fehlt oder ist tentativ zurueckgezogen. */
        MISSING
    }

    /**
     * Leitet die Identitaeten der Knoten des Erzeugungsplans von
     * {@code r} fuer den Match {@code refs} nach {@code ids} ab
     * (plan-indiziert, ein dynamisches Blatt ohne Quelle bekommt einen
     * Platzhalter, der nie gelesen wird) und meldet, was der Graph vom
     * Plan haelt (Rust {@code plan_of}). Jede Identitaet wird einmal
     * abgeleitet: fuer die Duplikat-Pruefung (wie
     * {@link #creationExists}), fuer die Tombstone-Pruefung und fuer die
     * Anwendung. Nach {@code TOMBSTONE} ist {@code ids} unvollstaendig.
     *
     * <p>Die Entscheidung ist die von 2.0.2: dort pruefte
     * {@code creationExists} bis zum ersten fehlenden Knoten und danach
     * die Schleife des V7-Guards bis zum ersten Tombstone, beide ueber
     * dieselben Identitaeten. Ein Tombstone irgendwo im Plan ist also
     * ein Widerspruch, sonst ein Duplikat, wenn alles lebendig da ist.
     */
    private Planned planOf(Graph g, Rule r, Id[] refs, ArrayList<Id> ids) {
        ids.clear();
        boolean allAlive = true;
        for (CreateNode cn : r.createNodes) {
            Id id = previewCreate(g, r, refs, ids, cn);
            if (id == null) { // dyn ohne Quelle: nichts würde erzeugt
                ids.add(cn.parent >= 0 ? refs[cn.parent] : ids.get(-cn.parent - 1));
                continue;
            }
            Node n = g.node(id);
            if (n != null && n.status == St.TOMBSTONE) return Planned.TOMBSTONE;
            if (n == null || !n.status.alive()) allAlive = false;
            ids.add(id);
        }
        if (!allAlive) return Planned.MISSING;
        for (int[] l : r.createLinks) {
            Id s = l[0] >= 0 ? refs[l[0]] : ids.get(-l[0] - 1);
            Id t = l[1] >= 0 ? refs[l[1]] : ids.get(-l[1] - 1);
            if (!g.connectedAlive(s, t)) return Planned.MISSING;
        }
        return Planned.EXISTS;
    }

    static final class Created {
        final List<Id> nodes, edges;
        Created(List<Id> nodes, List<Id> edges) { this.nodes = nodes; this.edges = edges; }
    }

    public Created applyCreation(Graph g, Rule r, Id[] refs) {
        return applyCreationAt(g, r, refs, null);
    }

    /**
     * {@link #applyCreation}, wahlweise mit den schon abgeleiteten
     * Identitaeten der Plan-Knoten ({@code ids}, plan-indiziert, wie
     * {@link #stepWithLimit} sie fuer seine Pruefungen ableitet). Rust
     * {@code apply_creation_at}. Der Eintrag eines dynamischen Blatts
     * ohne Quelle wird nicht gelesen.
     */
    Created applyCreationAt(Graph g, Rule r, Id[] refs, List<Id> ids) {
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
                id = ids != null ? g.addCorrAt(ids.get(planIx), cn.typ) : g.addCorr(parent, cn.typ, refs);
            } else if (cn.dynAttr != null) {
                Id src = g.childLeafOfType(refs[cn.dynAnchor], cn.dynAttr);
                if (src == null) { slots.add(null); continue; }
                id = ids != null ? g.addDerivedLeafAt(ids.get(planIx), cn.typ, src, cn.dynTransform)
                        : g.addDerivedLeaf(parent, cn.typ, src, cn.dynTransform);
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
                id = ids != null ? g.addKonstLeafAt(ids.get(planIx), cn.typ, cn.konst)
                        : g.addKonstLeaf(parent, cn.typ, r.name, planIx, cn.konst);
            } else if (cn.derivedLeaf < 0) {
                id = ids != null ? g.addGhostAt(ids.get(planIx), cn.typ) : g.addGhost(parent, cn.typ);
            } else {
                id = ids != null
                        ? g.addDerivedLeafAt(ids.get(planIx), cn.typ, refs[cn.derivedLeaf], cn.derivedTransform)
                        : g.addDerivedLeaf(parent, cn.typ, refs[cn.derivedLeaf], cn.derivedTransform);
            }
            // Die uebergebene Identitaet ist die, die hier abgeleitet
            // wuerde (slots haelt die Plan-Knoten vor diesem).
            assert ids == null || id.equals(previewCreate(g, r, refs, slots, cn)) : r.name + " " + planIx;
            if (cn.isCorr) g.markCorr(id);
            slots.add(id);
            created.add(id);
        }
        List<Id> edges = new ArrayList<>(r.createLinks.size());
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
        links.settleRemoved(g);
        while (true) {
            TodoKey k = null;
            if (ceiling == null && waveDirections == null) {
                // Ohne Filter: der erste Eintrag, entnommen in einem
                // Zugriff.
                k = todo.pollFirst();
            } else {
                if (ceiling == null) {
                    for (TodoKey c : todo) {
                        if (ruleActive(c.ruleIx)) { k = c; break; }
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
                if (k != null) todo.remove(k);
            }
            if (k == null) {
                return null;
            }
            Rule rule = rules.get(k.ruleIx);
            // Was der Schritt ueber den gezogenen Kandidaten weiss, haengt
            // an der Nummer seines Matches, die er traegt. Nachgeschlagen
            // wird ueber den Schluessel nur noch beim Vergessen eines
            // Matches mit tentativ zurueckgezogenem Anker.
            MatchKey mk = k.key;
            if (applied.contains(mk, k.mix)) continue;
            Recognised recognised = recognise(g, rule, k.refs);
            if (recognised == Recognised.TENTATIVE) {
                // TT-Anker feuern nicht (Reclaim-Zombie-Schutz): vergessener
                // Match wird vom nächsten Seed neu gefunden, falls resurrektiert.
                byKey.remove(mk);
                continue;
            }
            if (recognised == Recognised.TRANSLATED) {
                applied.insert(mk, k.mix);
                return false;
            }
            // Die Identitaeten der Plan-Knoten, einmal abgeleitet: fuer
            // die Duplikat-Pruefung, den V7-Guard und die Anwendung.
            Planned planned = planOf(g, rule, k.refs, ids);
            if (planned == Planned.EXISTS) {
                // Ebenfalls ein Duplikat: alles, was der Plan erzeugen
                // wuerde, ist da.
                applied.insert(mk, k.mix);
                return false;
            }
            if (planned == Planned.TOMBSTONE) {
                // V7-Guard: Tombstone-Substanz wiederzuverwenden
                // widerspricht der Historie (nicht TT, das ist die
                // legitime Resurrektionszone).
                sawContradiction = true;
                continue;
            }
            Created cr = applyCreationAt(g, rule, k.refs, ids);
            int eix = cascade.size();
            cascade.add(new Entry(k.ruleIx, rule.rank, k.refs, cr.nodes, cr.edges));
            cascadeLen++;
            // Provenienz, Produktseite: jeder Knoten, den dieser Eintrag
            // gebaut hat, zeigt auf ihn zurueck. Seine Kanten findet
            // producersOf aus ihren Enden.
            for (Id pid : cr.nodes) byProduct.computeIfAbsent(pid, x -> new IntList()).add(eix);
            if (byProductFiled != null) {
                for (Id pid : cr.edges) byProductFiled.computeIfAbsent(pid, x -> new IntList()).add(eix);
            }
            applied.insert(mk, k.mix);
            // Provenienz-Kante materialisieren (für retractFor ohne Scan).
            matches.get(pulledMatch(k)).entry = eix;
            expandAt(g, vals, cr.nodes);
            return true;
        }
    }

    /**
     * Der Match des gezogenen Kandidaten {@code k}: der, dessen Nummer er
     * traegt (Rust {@code pulled_match}). 2.0.2 schlug ihn ueber den
     * Schluessel in {@link #byKey} nach, unter {@link #checks} geschieht
     * das zusaetzlich, eine Abweichung wirft.
     */
    private int pulledMatch(TodoKey k) {
        if (checks) {
            Integer filed = byKey.get(k.key);
            if (filed == null || filed != k.mix) {
                throw new IllegalStateException("pulledMatch: der Kandidat traegt den Match " + k.mix
                        + ", unter seinem Schluessel steht " + filed + ", Regel " + rules.get(k.ruleIx).name);
            }
        }
        return k.mix;
    }

    /** Was der Graph ueber die Anker der Korrespondenzen sagt, die ein
     *  gezogener Kandidat stiften wuerde (Rust {@code Recognised}). */
    private enum Recognised {
        /** Ein Anker ist tentativ zurueckgezogen. */
        TENTATIVE,
        /** Jeder Anker traegt seine Korrespondenz schon
         *  ({@link Engine#isTranslated}). */
        TRANSLATED,
        /** Keins von beiden. */
        OPEN
    }

    /**
     * Ein Zugriff je Anker liefert seinen Status und seine Participations
     * (Rust {@code recognise}): die Frage, ob ein Anker tentativ
     * zurueckgezogen ist, und die Frage, ob er uebersetzt ist, lesen
     * denselben Platz des Graphen. Beide lesen nur den Graphen, Anker fuer
     * Anker gestellt geben sie also die Antworten, die erst die eine Frage
     * ueber alle Anker und dann die andere gaebe. Unter {@link #checks}
     * werden sie auch einzeln gestellt und verglichen, eine Abweichung
     * wirft.
     */
    private Recognised recognise(Graph g, Rule rule, Id[] refs) {
        boolean tentative = false;
        boolean translated = !rule.corrRecognition.isEmpty();
        for (Object[] rec : rule.corrRecognition) {
            Id anchor = refs[(Integer) rec[1]];
            Graph.Slot s = g.map.get(anchor);
            if (s == null) {
                // Kein Platz: nicht tentativ, ohne Participations nicht
                // uebersetzt.
                translated = false;
                continue;
            }
            if (s.node != null && s.node.status == St.TENTATIVE_TOMBSTONE) {
                tentative = true;
                break;
            }
            if (translated) {
                Integer t = g.lookup((String) rec[0]);
                Integer ept = g.lookup((String) rec[2]);
                translated = t != null && ept != null && s.parts != null
                        && anchorTranslatedIn(g, s.parts.ofType(t), ept, anchor);
            }
        }
        Recognised recognised = tentative ? Recognised.TENTATIVE
                : translated ? Recognised.TRANSLATED : Recognised.OPEN;
        if (checks) {
            boolean anyTt = false;
            for (Object[] rec : rule.corrRecognition) anyTt |= selfNodeTt(g, refs[(Integer) rec[1]]);
            Recognised expected = anyTt ? Recognised.TENTATIVE
                    : isTranslated(g, rule, refs) ? Recognised.TRANSLATED : Recognised.OPEN;
            if (recognised != expected) {
                throw new IllegalStateException("recognise: ein Gang ueber die Anker ergibt " + recognised
                        + ", die beiden Fragen einzeln " + expected + ", Regel " + rule.name);
            }
        }
        return recognised;
    }

    /** Ist {@code id} ein tentativ zurueckgezogener Knoten (Rust
     *  {@code self_node_tt})? */
    private static boolean selfNodeTt(Graph g, Id id) {
        Node n = g.node(id);
        return n != null && n.status == St.TENTATIVE_TOMBSTONE;
    }

    /** Traegt jeder Anker der Regel schon seine Korrespondenz mit einem
     *  lebendigen Gegenende (Rust {@code is_translated})? Nicht ohne
     *  Anker. */
    private static boolean isTranslated(Graph g, Rule rule, Id[] refs) {
        if (rule.corrRecognition.isEmpty()) return false;
        for (Object[] rec : rule.corrRecognition) {
            if (!anchorTranslated(g, (String) rec[0], (String) rec[2], refs[(Integer) rec[1]])) return false;
        }
        return true;
    }

    /** Traegt {@code anchor} schon eine lebendige Korrespondenz vom Typ
     *  {@code typ} mit einem lebendigen Gegenende vom Typ {@code endTyp}
     *  (Rust {@code anchor_translated})? */
    private static boolean anchorTranslated(Graph g, String typ, String endTyp, Id anchor) {
        Integer t = g.lookup(typ);
        Integer ept = g.lookup(endTyp);
        return t != null && ept != null && anchorTranslatedIn(g, g.partsOfType(anchor, t), ept, anchor);
    }

    /** {@link #anchorTranslated} fuer einen Anker, dessen Participations
     *  zur Hand sind (Rust {@code anchor_translated_in}): {@code ofType}
     *  sind die mit Korrespondenzen vom gesuchten Typ, {@code ept} ist der
     *  Typ des Gegenendes. */
    private static boolean anchorTranslatedIn(Graph g, Iterable<Part> ofType, int ept, Id anchor) {
        for (Part p : ofType) {
            Conn c = g.conn(p.connection);
            Node other = g.node(p.other);
            if (c == null || !c.status.alive() || other == null || !other.status.alive()) continue;
            for (Part q : g.parts(p.other)) {
                if (!q.other.equals(anchor) && q.otherTyp == ept) {
                    Node en = g.node(q.other);
                    if (en != null && en.status.alive()) return true;
                }
            }
        }
        return false;
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

    /**
     * Die Schluessel der angewandten Matches, in der Folge der Matches.
     * Jeder steht einmal darin: eine Marke traegt nur der Match, der unter
     * seinem Schluessel in {@link #byKey} steht ({@link Applied}). Fuer
     * Tests und den Abzug in {@code Worlds.dump}, der in 2.0.2 die Menge
     * {@code applied} las.
     */
    List<MatchKey> appliedKeys() {
        List<MatchKey> out = new ArrayList<>();
        for (int mix = 0; mix < matches.size(); mix++) {
            if (applied.marked(mix)) out.add(matches.get(mix).key);
        }
        return out;
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
