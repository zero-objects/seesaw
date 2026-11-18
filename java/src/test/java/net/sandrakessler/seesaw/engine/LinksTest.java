package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Die Matches, die auf einer Verbindung ruhen ({@link Links}). Die Tests
 * mit Namen in Rust-Schreibweise sind portiert aus {@code src/engine/mod.rs}
 * (Rust 87b270a, 1cb8253, b9b3ce9) und heissen wie dort. Unter
 * {@code -Dseesaw.checks=true} wird jede Antwort von
 * {@link Links#filedUnder} mit der Ablage von 2.0.2 verglichen.
 *
 * <p>Java hat kein {@code Graph.clone}. Wo der Rust-Test mit einer Kopie
 * weitermacht, nimmt der Java-Test die materialisierte Kopie
 * ({@link Graph#materialize}).
 */
final class LinksTest {

    /**
     * Zwei Regeln (Rust {@code context_link_rules}): {@code Root} setzt ein
     * {@code A} mit einem {@code B} in Beziehung, {@code Child} ein
     * {@code X} unter dem {@code A} mit einem {@code Y} unter dem {@code B}
     * und liest die Korrespondenz von {@code Root} als Kontext. Das Muster
     * von {@code Child} nennt den Link von dieser Korrespondenz zum
     * {@code A}, der Graph haelt die Verbindung andersherum.
     */
    private static final String CONTEXT_LINK_RULES = """
            {
              "format": 3,
              "name": "context_link",
              "rules": [
                {
                  "name": "Root", "rank": 100,
                  "left": {"anchor": "a", "nodes": [
                    {"name": "a", "type": "A"}
                  ]},
                  "right": {"anchor": "b", "nodes": [
                    {"name": "b", "type": "B"}
                  ]},
                  "corrs": [{"type": "AB", "left": "a", "right": "b",
                             "role": "establishes"}]
                },
                {
                  "name": "Child", "rank": 90,
                  "left": {"anchor": "a", "nodes": [
                    {"name": "a", "type": "A"},
                    {"name": "x", "type": "X"}
                  ], "links": [["a", "x"]]},
                  "right": {"anchor": "b", "nodes": [
                    {"name": "b", "type": "B"},
                    {"name": "y", "type": "Y"}
                  ], "links": [["b", "y"]]},
                  "corrs": [
                    {"type": "AB", "left": "a", "right": "b",
                     "role": "references"},
                    {"type": "XY", "left": "x", "right": "y",
                     "role": "establishes"}
                  ]
                }
              ]
            }
            """;

    /** Graph, Knoten {@code a}, seine Kinder und die Regeln
     *  (Rust {@code context_link_graph}). */
    record ContextLink(Graph g, Id a, List<Id> xs, List<Rule> rules) {}

    /** {@code a} mit {@code n} Kindern vom Typ {@code X}, die Regeln von
     *  {@link #CONTEXT_LINK_RULES} darin geladen. */
    static ContextLink contextLinkGraph(int n) {
        Graph g = new Graph();
        Id a = g.addBaseline("a", "A");
        List<Id> xs = new ArrayList<>();
        for (int i = 0; i < n; i++) {
            Id x = g.addBaseline("a/x" + i, "X");
            g.connect(a, x, St.SOLID);
            xs.add(x);
        }
        List<Rule> rules = Rules.load(CONTEXT_LINK_RULES, g);
        return new ContextLink(g, a, xs, rules);
    }

    /** Eine Engine fuer eine Welle von der Quellseite. */
    static Engine forward(List<Rule> rules) {
        Engine e = new Engine(rules);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        return e;
    }

    /** Die Korrespondenz vom Typ {@code AB}, die {@code Root} erzeugt. */
    static Id corrAb(Graph g) {
        Integer ab = g.lookup("AB");
        assertNotNull(ab, "Typ AB");
        Id found = null;
        for (Node n : g.allNodes()) {
            if (n.isCorr && n.typ == ab) {
                assertNull(found, "eine Korrespondenz AB");
                found = n.id;
            }
        }
        assertNotNull(found, "AB");
        return found;
    }

    /** Schritte ohne neuen Seed, bis keiner mehr geht (Rust
     *  {@code run_remaining}). */
    static void runRemaining(Engine e, Graph g, Map<Id, String> vals) {
        for (int i = 0; i < 2000; i++) {
            if (e.step(g, vals) == null) return;
        }
        throw new AssertionError("kein Ende nach 2000 Schritten");
    }

    private static int size(IntList l) {
        return l == null ? 0 : l.size();
    }

    private static int[] array(IntList l) {
        return l == null ? new int[0] : l.toArray();
    }

    /** Der Graph, auf den sich der Index der nicht gehaltenen Links
     *  bezieht, null ohne Index. */
    private static Long indexGraph(Engine e) {
        return e.links.unheld == null ? null : e.links.unheld.graph;
    }

    /** Wie viele Matches der Index der nicht gehaltenen Links unter
     *  {@code id} haelt. */
    private static int inUnheld(Engine e, Id id) {
        return e.links.unheld == null ? 0 : size(e.links.unheld.byId.get(id));
    }

    /**
     * Ein Match ruht auf der Verbindung jedes seiner Links in der Richtung,
     * die das Muster nennt, auch wo der Graph die Verbindung andersherum
     * haelt (ein Link zu einer Kontext-Korrespondenz ist richtungsfrei).
     * Ein Element mit dieser Id zu entfernen, die der Graph nicht kennt,
     * toetet den Match.
     */
    @Test
    void a_match_rests_on_the_named_direction_of_a_context_link() {
        Graph g = new Graph();
        Id a = g.addBaseline("a", "A");
        Id x = g.addBaseline("a/x", "X");
        g.connect(a, x, St.SOLID);
        List<Rule> rules = Rules.load(CONTEXT_LINK_RULES, g);
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(rules);
        e.seed(g, vs);
        // Root wird angewandt und erzeugt die Korrespondenz. Der Match von
        // Child wird von ihr aus gefunden und wartet im Todo.
        assertEquals(Boolean.TRUE, e.step(g, vs));
        assertEquals(1, e.todo.size(), "der Kandidat von Child");
        Id corr = corrAb(g);
        // Der Graph haelt a → corr. Das Muster von Child nennt den Link
        // von der Korrespondenz zu ihrem Anker.
        assertNotNull(g.connectionId(a, corr));
        Id named = Ident.identConnection(corr, a);
        assertTrue(g.conn(named) == null && g.node(named) == null);
        e.elementRemoved(named);
        assertNull(e.step(g, vs), "der Kandidat von Child ist tot");
        assertEquals(1, e.cascade.size());
    }

    /**
     * Eine Id, die der Graph nicht kennt, wird aus dem Index der nicht
     * gehaltenen Links beantwortet. Der Index entsteht bei der ersten
     * solchen Frage, {@code record} fuehrt ihn weiter, fuer einen anderen
     * Graphen wird er neu gebaut. Eine zweimal vorgemerkte Id toetet
     * einmal.
     */
    @Test
    void an_unknown_id_is_answered_from_the_unheld_links() {
        ContextLink w = contextLinkGraph(3);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(3, e.todo.size(), "die Kandidaten von Child");
        Id corr = corrAb(g);
        assertNull(e.links.unheld, "keine Frage bisher, kein Index");
        // Eine Id, auf der nichts ruht: die Frage baut den Index und
        // toetet nichts.
        Id nothing = Ident.identConnection(xs.get(0), xs.get(1));
        e.elementRemoved(nothing);
        assertEquals(Boolean.TRUE, e.step(g, vs), "ein Child");
        assertEquals(g.uid(), indexGraph(e), "ein Index fuer diesen Graphen");
        assertEquals(2, e.todo.size());
        // Ein Match, der von hier an eingetragen wird, steht im Index.
        Id x3 = g.addBaseline("a/x3", "X");
        g.connect(a, x3, St.SOLID);
        e.elementsAdded(g, vs, List.of(x3));
        assertEquals(3, e.todo.size());
        Id named = Ident.identConnection(corr, a);
        assertEquals(4, inUnheld(e, named), "jeder Match von Child ruht auf dem genannten Link");
        // Der Host macht mit einer Kopie weiter: der Index gilt dem anderen
        // Graphen und faellt weg, die naechste Frage baut einen fuer die
        // Kopie.
        Graph copy = g.materialize();
        Id x4 = copy.addBaseline("a/x4", "X");
        copy.connect(a, x4, St.SOLID);
        e.elementsAdded(copy, vs, List.of(x4));
        assertNull(e.links.unheld, "der Index galt dem anderen Graphen");
        assertEquals(4, e.todo.size());
        e.elementRemoved(named);
        e.elementRemoved(named);
        assertNull(e.step(copy, vs), "die Kandidaten von Child sind tot");
        assertEquals(copy.uid(), indexGraph(e), "ein Index fuer die Kopie");
        assertEquals(5, inUnheld(e, named));
        assertEquals(2, e.cascade.size());
    }

    /**
     * Ein Graph, den der Host durch seine materialisierte Kopie ersetzt
     * hat: die Kopie laesst die Tombstones weg, und eine spaetere
     * Retraktion fragt nach ihnen.
     */
    @Test
    void an_id_the_materialized_copy_left_out_is_answered() {
        ContextLink w = contextLinkGraph(3);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        runRemaining(e, g, vs);
        assertEquals(4, e.cascade.size(), "Root und drei Child");
        // x0 geht: was sein Child erzeugt hat, wird zurueckgezogen und
        // wird Tombstone.
        g.setNodeStatus(xs.get(0), St.TOMBSTONE);
        e.elementDeleted(g, xs.get(0));
        e.consolidate(g);
        List<Id> gone = new ArrayList<>();
        for (Engine.Entry entry : e.cascade.subList(1, e.cascade.size())) {
            for (Id id : entry.created) {
                Node n = g.node(id);
                if (n != null && n.status == St.TOMBSTONE) gone.add(id);
            }
        }
        assertFalse(gone.isEmpty(), "die Erzeugnisse eines Child sind Tombstones");
        Graph copy = g.materialize();
        for (Id id : gone) assertNull(copy.node(id));
        // a geht. Die Retraktion kommt wieder am toten Match von x0 vorbei
        // und fragt nach seinen Erzeugnissen, die die Kopie nicht haelt.
        copy.setNodeStatus(a, St.TOMBSTONE);
        e.elementDeleted(copy, a);
        e.consolidate(copy);
        assertEquals(copy.uid(), indexGraph(e), "ein Index fuer die Kopie");
    }

    /**
     * Der Index bezieht sich auf den Graphen, nicht auf seine Typtabelle.
     * Ein Host, der die Typtabelle in einen anderen Graphen setzt, nimmt
     * sie mit. Der Index, der fuer den ersten Graphen gebaut wurde, darf
     * nicht fuer den zweiten antworten.
     *
     * <p>In Rust ist {@code types} ein oeffentliches Feld und wird per Move
     * zugewiesen. In Java ist die Typtabelle das oeffentliche Feld
     * {@code typesByName}, es ist {@code final}, die Zuweisung geht hier
     * ueber Reflexion.
     */
    @Test
    void the_index_is_about_the_graph_not_its_type_table() throws ReflectiveOperationException {
        ContextLink w = contextLinkGraph(2);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(2, e.todo.size(), "die Kandidaten von Child");
        // Die erste Frage baut den Index fuer g. g haelt a → x0, dieser
        // Link steht also nicht darin.
        Id nothing = Ident.identConnection(xs.get(0), xs.get(1));
        assertNull(e.links.filedUnder(g, nothing));
        Id held = g.connectionId(a, xs.get(0));
        assertNotNull(held, "a → x0");
        assertEquals(0, inUnheld(e, held));
        // Ein anderer Graph mit der Typtabelle von g: dieselben
        // Baseline-Knoten, ohne a → x0.
        Graph h = new Graph();
        Field types = Graph.class.getField("typesByName");
        types.setAccessible(true);
        types.set(h, g.typesByName);
        types.set(g, new LinkedHashMap<String, Integer>());
        assertEquals(a, h.addBaseline("a", "A"));
        assertEquals(xs.get(0), h.addBaseline("a/x0", "X"));
        assertEquals(xs.get(1), h.addBaseline("a/x1", "X"));
        h.connect(a, xs.get(1), St.SOLID);
        assertTrue(h.conn(held) == null && h.node(held) == null);
        // Wie in 2.0.2: der Match von Child ueber x0 ruht auf a → x0.
        IntList resting = e.links.filedUnder(h, held);
        assertEquals(1, size(resting), "der Match von Child ueber x0");
        assertEquals(h.uid(), indexGraph(e), "ein Index fuer h");
    }

    /** Ein Match, der nach dem Bau des Index eingetragen wird, steht darin,
     *  und eine Frage nach der Id wird aus ihm beantwortet. */
    @Test
    void a_match_recorded_after_the_index_is_answered_from_it() {
        ContextLink w = contextLinkGraph(2);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        Id corr = corrAb(g);
        Id named = Ident.identConnection(corr, a);
        Id nothing = Ident.identConnection(xs.get(0), xs.get(1));
        assertNull(e.links.filedUnder(g, nothing));
        assertEquals(2, inUnheld(e, named));
        Id x = g.addBaseline("a/x2", "X");
        g.connect(a, x, St.SOLID);
        e.elementsAdded(g, vs, List.of(x));
        assertEquals(3, e.todo.size(), "die Kandidaten von Child");
        assertEquals(3, inUnheld(e, named), "der neue Match steht dort durch record");
        IntList resting = e.links.filedUnder(g, named);
        assertEquals(3, size(resting), "jeder Match von Child");
        e.elementRemoved(named);
        assertNull(e.step(g, vs), "die Kandidaten von Child sind tot");
        assertEquals(1, e.cascade.size(), "nur Root");
    }

    /**
     * Die Verbindung eines Links aus dem Index entsteht spaeter, in der
     * Richtung des Links. Die Id ist jetzt bekannt: die Matches kommen aus
     * ihren Enden, und die Antwort ist dieselbe.
     */
    @Test
    void a_connection_that_comes_later_gives_the_same_answer() {
        ContextLink w = contextLinkGraph(2);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        Id corr = corrAb(g);
        Id named = Ident.identConnection(corr, a);
        Id nothing = Ident.identConnection(xs.get(0), xs.get(1));
        assertNull(e.links.filedUnder(g, nothing));
        assertEquals(2, inUnheld(e, named));
        g.connect(corr, a, St.SOLID);
        assertNotNull(g.conn(named), "corr → a wird jetzt gehalten");
        IntList resting = e.links.filedUnder(g, named);
        assertEquals(2, size(resting), "beide Matches von Child");
        e.elementRemoved(named);
        assertNull(e.step(g, vs), "die Kandidaten von Child sind tot");
        assertEquals(1, e.cascade.size(), "nur Root");
    }

    /**
     * Die materialisierte Kopie laesst eine Verbindung weg, die Tombstone
     * war. Der Match, der auf ihr ruht, wird ueber den Index fuer die Kopie
     * gefunden und stirbt, die anderen werden angewandt.
     */
    @Test
    void a_connection_the_copy_left_out_kills_the_match_resting_on_it() {
        ContextLink w = contextLinkGraph(3);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(3, e.todo.size(), "die Kandidaten von Child");
        Id link = g.connectionId(a, xs.get(0));
        assertNotNull(link, "a → x0");
        g.setConnectionStatus(link, St.TOMBSTONE);
        Graph copy = g.materialize();
        assertTrue(copy.conn(link) == null && copy.node(link) == null);
        e.elementRemoved(link);
        runRemaining(e, copy, vs);
        assertEquals(copy.uid(), indexGraph(e), "ein Index fuer die Kopie");
        assertEquals(3, e.cascade.size(), "Root und das Child von x1 und x2");
        assertEquals(0, corrsAt(copy, xs.get(0)), "das Child von x0 ist tot");
        assertEquals(1, corrsAt(copy, xs.get(1)));
        assertEquals(1, corrsAt(copy, xs.get(2)));
    }

    /** Wie viele Korrespondenzen an {@code x} haengen. */
    static int corrsAt(Graph g, Id x) {
        int n = 0;
        for (Part p : g.parts(x)) {
            Node o = g.node(p.other);
            if (o != null && o.isCorr) n++;
        }
        return n;
    }

    /**
     * Ein Muster, das einen Link zweimal hat (ein Host kann einen in eine
     * geladene Regel schieben): der Match ruht zweimal auf der Verbindung,
     * aus den Enden wie aus dem Index.
     */
    @Test
    void a_link_twice_in_a_pattern_files_the_match_twice() {
        ContextLink w = contextLinkGraph(2);
        Graph g = w.g();
        Id a = w.a();
        List<Id> xs = w.xs();
        for (Rule r : w.rules()) {
            List<PatLink> directed = new ArrayList<>();
            for (PatLink l : r.patLinks) {
                if (l.kind == PatLink.Kind.DIRECTED) directed.add(l);
            }
            r.patLinks.addAll(directed);
        }
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(2, e.todo.size(), "die Kandidaten von Child");
        Id link = g.connectionId(a, xs.get(0));
        assertNotNull(link, "a → x0");
        int[] fromEnds = array(e.links.filedUnder(g, link));
        assertEquals(2, fromEnds.length, "ein Match, zweimal");
        assertEquals(fromEnds[0], fromEnds[1]);
        g.setConnectionStatus(link, St.TOMBSTONE);
        Graph copy = g.materialize();
        assertTrue(copy.conn(link) == null && copy.node(link) == null);
        int[] fromIndex = array(e.links.filedUnder(copy, link));
        assertArrayEquals(fromEnds, fromIndex);
    }

    /**
     * Java hat mit {@link Engine#record(int, Id[])} eine oeffentliche Tuer,
     * die ohne Graph einen Match eintraegt. Zwischen einem
     * {@link Engine#elementRemoved}, das eine Id vormerkt, und der naechsten
     * Tuer mit Graph eingetragen, ruht der neue Match auf der vorgemerkten
     * Verbindung. 2.0.2 toetete beim Aufruf nur die Matches, die schon
     * eingetragen waren: der neue lebt und wird angewandt.
     */
    @Test
    void einOhneGraphEingetragenerMatchUeberlebtDieVormerkung() {
        ContextLink w = contextLinkGraph(1);
        Graph g = w.g();
        Id a = w.a();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(1, e.todo.size(), "der Kandidat von Child ueber x0");
        Engine.TodoKey child = e.todo.first();
        Id corr = corrAb(g);
        Id named = Ident.identConnection(corr, a);
        // Ein weiteres Kind, eingetragen ohne Graph: die Ref-Folge des
        // Kandidaten mit dem neuen Kind an der Stelle von x0.
        Id x0 = w.xs().get(0);
        Id x1 = g.addBaseline("a/x1", "X");
        g.connect(a, x1, St.SOLID);
        Id[] refs = child.refs.clone();
        int at = -1;
        for (int i = 0; i < refs.length; i++) {
            if (refs[i].equals(x0)) at = i;
        }
        assertTrue(at >= 0, "x0 in der Ref-Folge");
        refs[at] = x1;
        e.elementRemoved(named);
        e.record(child.ruleIx, refs);
        assertEquals(2, e.todo.size(), "vorgemerkt, noch nichts getoetet");
        assertEquals(Boolean.TRUE, e.step(g, vs), "das Child von x1");
        assertNull(e.step(g, vs), "das Child von x0 ist tot");
        assertEquals(2, e.cascade.size());
        assertEquals(1, corrsAt(g, x1));
        assertEquals(0, corrsAt(g, x0));
    }

    /** Die Stelle von {@code x} in der Ref-Folge des Kandidaten. */
    private static int at(Engine.TodoKey k, Id x) {
        for (int i = 0; i < k.refs.length; i++) {
            if (k.refs[i].equals(x)) return i;
        }
        throw new AssertionError("nicht in der Ref-Folge");
    }

    /**
     * Ein Match hat seinen Schluessel verloren: sein Anker war tentativ
     * zurueckgezogen, als sein Kandidat gezogen wurde. Er lebt und steht
     * nicht im Todo. Dann {@link Engine#elementRemoved} fuer eine
     * Verbindung, auf der er ruht (vorgemerkt), und
     * {@link Engine#record(int, Id[])} mit demselben Schluessel: ein neuer
     * Match mit Kandidat. Die naechste Tuer mit Graph toetet den alten
     * Match, der Kandidat des neuen bleibt. 2.0.2 toetete den alten Match
     * beim Aufruf, der neue wurde angewandt.
     */
    @Test
    void derKandidatEinesSpaeterEingetragenenMatchesBleibt() {
        ContextLink w = contextLinkGraph(1);
        Graph g = w.g();
        Id a = w.a();
        Id x0 = w.xs().get(0);
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        assertEquals(1, e.todo.size(), "der Kandidat von Child ueber x0");
        Engine.TodoKey child = e.todo.first();
        g.setNodeStatus(x0, St.TENTATIVE_TOMBSTONE);
        assertNull(e.step(g, vs), "der Anker ist tentativ zurueckgezogen");
        assertFalse(e.matches.get(child.mix).dead, "der Match lebt");
        assertNull(e.byKey.get(child.key), "ohne Schluessel");
        assertTrue(e.todo.isEmpty());
        g.setNodeStatus(x0, St.SOLID);
        Id named = Ident.identConnection(corrAb(g), a);
        e.elementRemoved(named);
        e.record(child.ruleIx, child.refs.clone());
        assertEquals(1, e.todo.size(), "der Kandidat des neuen Matches");
        assertEquals(Boolean.TRUE, e.step(g, vs), "der neue Match wird angewandt");
        assertTrue(e.matches.get(child.mix).dead, "der alte Match ist tot");
        assertEquals(2, e.cascade.size());
        assertEquals(1, corrsAt(g, x0));
    }

    /**
     * {@link Engine#record(int, Id[])} bei bestehendem Index der nicht
     * gehaltenen Links: ohne Graph ist offen, welche Verbindungen gehalten
     * werden, der Index faellt weg. Die naechste Frage baut ihn fuer ihren
     * Graphen neu, und der neue Match steht darin. Unter
     * {@code -Dseesaw.checks=true} vergleicht {@link Links#filedUnder}
     * jede Antwort mit der Ablage von 2.0.2.
     */
    @Test
    void einOhneGraphEingetragenerMatchVerwirftDenIndex() {
        ContextLink w = contextLinkGraph(1);
        Graph g = w.g();
        Id a = w.a();
        Map<Id, String> vs = new HashMap<>();
        Engine e = forward(w.rules());
        e.seed(g, vs);
        assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
        Engine.TodoKey child = e.todo.first();
        Id named = Ident.identConnection(corrAb(g), a);
        assertEquals(1, size(e.links.filedUnder(g, named)), "der Match von Child ueber x0");
        assertEquals(g.uid(), indexGraph(e), "ein Index fuer g");
        Id x1 = g.addBaseline("a/x1", "X");
        g.connect(a, x1, St.SOLID);
        Id[] refs = child.refs.clone();
        refs[at(child, w.xs().get(0))] = x1;
        e.record(child.ruleIx, refs);
        assertNull(e.links.unheld, "ohne Graph faellt der Index weg");
        int[] resting = array(e.links.filedUnder(g, named));
        assertEquals(g.uid(), indexGraph(e), "neu gebaut fuer g");
        assertArrayEquals(new int[] {child.mix, e.matches.size() - 1}, resting,
                "beide Matches von Child, der neue darunter");
        e.elementRemoved(named);
        assertNull(e.step(g, vs), "die Kandidaten von Child sind tot");
        assertEquals(1, e.cascade.size(), "nur Root");
    }

    /**
     * Ein Host ruft das oeffentliche {@link Engine#retractMatch} fuer einen
     * toten Match, dessen Schluessel inzwischen ein anderer Match haelt. Der
     * Aufruf nimmt dem anderen den Schluessel, dessen Kandidat bleibt im
     * Todo und traegt keinen Match mehr (Nummer -1). {@link Engine#record}
     * mit demselben Schluessel ersetzt ihn durch den Kandidaten des neuen
     * Matches. Mit und ohne Pruefung gleich.
     */
    @Test
    void einKandidatOhneMatchWirdVomNeuenErsetzt() {
        for (boolean checks : new boolean[] {true, false}) {
            ContextLink w = contextLinkGraph(1);
            Graph g = w.g();
            Map<Id, String> vs = new HashMap<>();
            Engine e = new Engine(w.rules(), Engine.defaultUseIndex(), checks);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.seed(g, vs);
            assertEquals(Boolean.TRUE, e.step(g, vs), "Root");
            Engine.TodoKey child = e.todo.first();
            e.elementRemoved(w.xs().get(0));
            assertTrue(e.matches.get(child.mix).dead);
            assertTrue(e.todo.isEmpty());
            // Der tote Match gibt seinen Schluessel her, ein neuer Match
            // bekommt ihn, der tote nimmt ihn dem neuen.
            e.retractMatch(g, child.mix, new ArrayList<>());
            e.record(child.ruleIx, child.refs.clone());
            int other = e.matches.size() - 1;
            assertEquals(other, e.todo.first().mix, "der Kandidat des anderen");
            e.retractMatch(g, child.mix, new ArrayList<>());
            assertNull(e.byKey.get(child.key));
            assertEquals(1, e.todo.size());
            assertEquals(-1, e.todo.first().mix, "der Kandidat traegt keinen Match");
            assertFalse(e.matches.get(other).dead, "der andere lebt");
            e.record(child.ruleIx, child.refs.clone());
            int third = e.matches.size() - 1;
            assertEquals(Integer.valueOf(third), e.byKey.get(child.key));
            assertEquals(1, e.todo.size());
            assertEquals(third, e.todo.first().mix, "der Kandidat des neuen Matches");
        }
    }
}
