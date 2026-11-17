package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Die beiden Wege der Erzeugung: {@link Engine#applyCreation}, die
 * öffentliche Tür von 2.0.2, die jede Identität selbst ableitet und jeden
 * Namen in den Tabellen des Graphen auflöst, und
 * {@link Engine#applyCreationAt} mit den vorab abgeleiteten Identitäten
 * und den Kennungen aus {@link Names}, der Weg des Schritts. Beide laufen
 * auf zwei gleich aufgebauten Graphen mit denselben Matches. Nach jeder
 * Anwendung sind Knoten, Verbindungen, Beteiligungslisten und Typtabelle
 * der beiden Graphen gleich, ebenso die erzeugten Knoten und Verbindungen.
 */
final class ApplyCreationTest {

    /** Drei Regeln im Regelformat: abgeleitetes Blatt mit statischer
     *  Bindung und Korrespondenz ({@code Father_2_Male}), ein
     *  Konstanten-Blatt ({@code Num_Seq}), ein dynamisches Blatt, dessen
     *  Quelle fehlen kann ({@code Class_Dyn}). */
    private static final String RULES = """
        {"format":3,"name":"apply_creation","rules":[
          {"name":"Father_2_Male","rank":850,
           "left":{"anchor":"fam","nodes":[
             {"name":"fam","type":"Family"},
             {"name":"father","type":"Father"},
             {"name":"member","type":"Member"},
             {"name":"first","type":"firstName"}
           ],"links":[["fam","father"],["father","member"],["member","first"]]},
           "right":{"anchor":"male","nodes":[
             {"name":"male","type":"Male"},
             {"name":"name","type":"name"}
           ],"links":[["male","name"]]},
           "corrs":[
             {"type":"PersonCorr","left":"member","right":"male","role":"establishes",
              "bindings":[{"left":"first","right":"name"}]}
           ]},
          {"name":"Num_Seq","rank":800,
           "left":{"anchor":"sel","nodes":[{"name":"sel","type":"SelectStatement"}]},
           "right":{"anchor":"ann","nodes":[
             {"name":"ann","type":"Annotation"},
             {"name":"strat","type":"strategy",
              "predicate":{"kind":"equals","value":"sequential"},
              "constant":"sequential"}
           ],"links":[["ann","strat"]]},
           "corrs":[{"type":"NumCorr","left":"sel","right":"ann","role":"establishes"}]},
          {"name":"Class_Dyn","rank":40,
           "left":{"anchor":"cls","nodes":[
             {"name":"model","type":"Model"},
             {"name":"cls","type":"Class"}
           ],"links":[["model","cls"]]},
           "right":{"anchor":"jcls","nodes":[
             {"name":"jmodel","type":"Model","same_as":"model"},
             {"name":"jcls","type":"JavaClass"}
           ],"links":[["jmodel","jcls"]]},
           "corrs":[{"type":"CorrClass","left":"cls","right":"jcls","role":"establishes",
                     "bindings":[{"left_type":"name","right_type":"name","transform":[]}]}]}
        ]}""";

    /**
     * Eine Regel von Hand, mit den Arten, die das Regelformat nicht
     * erzeugt: eine Korrespondenz mit der Identität des ganzen Matches
     * ({@code corrFullMatch}), darunter ein dynamisches Blatt mit
     * Attribut-Korrespondenz, darunter ein Knoten, dessen Eltern-Platz
     * leer bleibt, wenn die Quelle fehlt, und ein Konstanten-Blatt an der
     * Korrespondenz.
     */
    private static Rule hand(Graph g) {
        Rule r = new Rule();
        r.name = "Hand";
        r.rank = 30;
        r.patNodes = List.of(new PatNode(g.intern("Class"), null));
        r.patLinks = List.of();
        r.createNodes = new ArrayList<>();
        r.createNodes.add(new CreateNode("HandCorr", 0, -1, null, null, -1, null, null, true, true));
        r.createNodes.add(new CreateNode("label", -1, -1, null, null, 0, "name", Chain.IDENTITY,
                false, false, "HandAttr"));
        r.createNodes.add(new CreateNode("Sub", -2, -1, null));
        r.createNodes.add(new CreateNode("flag", -1, -1, null, "on"));
        r.createLinks = new ArrayList<>();
        r.createLinks.add(new int[] {0, -1});
        r.createLinks.add(new int[] {-1, -2});
        r.createLinks.add(new int[] {-2, -3});
        r.createLinks.add(new int[] {-1, -4});
        r.inputTypes = List.of("Class");
        return r;
    }

    /** Familien, Anweisungen select und Klassen, die Hälfte der Klassen
     *  ohne Namen. Für beide Graphen gleich. */
    private static Graph welt(Map<Id, String> vals) {
        Graph g = new Graph();
        for (int i = 0; i < 3; i++) {
            Id f = g.addBaseline("f" + i, "Family");
            Id r = g.addBaseline("f" + i + "/father", "Father");
            Id m = g.addBaseline("f" + i + "/father/m", "Member");
            Id leaf = g.addBaseline("f" + i + "/father/m/fn", "firstName");
            g.connect(f, r, St.SOLID);
            g.connect(r, m, St.SOLID);
            g.connect(m, leaf, St.SOLID);
            vals.put(leaf, "John" + i);
        }
        g.addBaseline("sel0", "SelectStatement");
        g.addBaseline("sel1", "SelectStatement");
        Id model = g.addBaseline("m", "Model");
        for (int i = 0; i < 4; i++) {
            Id c = g.addBaseline("m/c" + i, "Class");
            g.connect(model, c, St.SOLID);
            if (i % 2 == 0) {
                Id name = g.addBaseline("m/c" + i + "/name", "name");
                g.connect(c, name, St.SOLID);
                vals.put(name, "Klasse" + i);
            }
        }
        return g;
    }

    private static List<Rule> rules(Graph g) {
        List<Rule> rules = new ArrayList<>(Rules.load(RULES, g));
        rules.add(hand(g));
        return rules;
    }

    /**
     * Die Identitäten der Plan-Knoten wie {@code Engine.planOf} sie
     * ableitet: plan-indiziert, ein dynamisches Blatt ohne Quelle bekommt
     * den Platz seines Eltern-Knotens, der nie gelesen wird.
     */
    private static List<Id> ids(Engine e, Graph g, Rule r, Id[] refs) {
        List<Id> ids = new ArrayList<>();
        for (CreateNode cn : r.createNodes) {
            Id id = e.previewCreate(g, r, refs, ids, cn);
            if (id == null) id = cn.parent >= 0 ? refs[cn.parent] : ids.get(-cn.parent - 1);
            ids.add(id);
        }
        return ids;
    }

    private static String hex(Id id) {
        return id == null ? "-" : HexFormat.of().formatHex(id.b);
    }

    /** Knoten mit allen Feldern und ihren Beteiligungslisten in deren
     *  Ordnung, Verbindungen, Typtabelle und die Knoten je Typ in der
     *  Folge des Einfügens. */
    private static String abzug(Graph g, Map<Id, String> vals) {
        StringBuilder out = new StringBuilder();
        out.append("typen ").append(g.typesByName).append('\n');
        for (int t = 0; t < g.typesByName.size(); t++) {
            out.append("typ ").append(t);
            for (Id id : g.idsOfType(t)) out.append(' ').append(hex(id));
            out.append('\n');
        }
        TreeMap<Id, Node> nodes = new TreeMap<>();
        for (Node n : g.allNodes()) nodes.put(n.id, n);
        for (Node n : nodes.values()) {
            out.append("knoten ").append(hex(n.id)).append(' ').append(n.typ).append(' ')
                    .append(n.status).append(" corr=").append(n.isCorr).append(" quelle=")
                    .append(hex(n.derivedSource)).append(" kette=").append(n.derivedTransform)
                    .append(" konst=").append(n.konstIx).append(" wert=")
                    .append(g.resolveValue(n.id, vals)).append('\n');
            for (Part p : g.parts(n.id)) {
                out.append("  teil ").append(p.otherTyp).append(' ').append(hex(p.other)).append(' ')
                        .append(p.outgoing).append(' ').append(hex(p.connection)).append('\n');
            }
        }
        TreeMap<Id, Conn> conns = new TreeMap<>();
        for (Conn c : g.allConnections()) conns.put(c.id, c);
        for (Conn c : conns.values()) {
            out.append("verbindung ").append(hex(c.id)).append(' ').append(hex(c.source)).append(' ')
                    .append(hex(c.target)).append(' ').append(c.status).append('\n');
        }
        return out.toString();
    }

    private static String ids(List<Id> ids) {
        StringBuilder out = new StringBuilder();
        for (Id id : ids) out.append(hex(id)).append(' ');
        return out.toString();
    }

    @Test
    void beideWegeBauenDenselbenGraphen() {
        Map<Id, String> vals = new HashMap<>();
        Graph alt = welt(vals);
        Graph neu = welt(new HashMap<>());
        List<Rule> altRules = rules(alt);
        List<Rule> neuRules = rules(neu);
        assertEquals(abzug(alt, vals), abzug(neu, vals), "gleich aufgebaut");
        Engine altEngine = new Engine(altRules);
        Engine neuEngine = new Engine(neuRules);
        Names names = new Names();
        // Wie oft jede Art eines Plan-Knotens angewandt wurde.
        Map<String, Integer> arten = new TreeMap<>();
        int anwendungen = 0;
        // Zweimal: im zweiten Durchgang ist alles da, die Erzeugung nimmt
        // die Knoten und Verbindungen wieder, die es gibt.
        for (int runde = 0; runde < 2; runde++) {
            for (int ri = 0; ri < altRules.size(); ri++) {
                Rule a = altRules.get(ri);
                Rule n = neuRules.get(ri);
                List<Id[]> matches = Matcher.findMatchesWithFixed(alt, vals, a,
                        new Id[a.patNodes.size()]);
                for (Id[] refs : matches) {
                    String wo = "Runde " + runde + ", Regel " + a.name + ", Match "
                            + Engine.key(ri, refs);
                    arten(arten, alt, a, refs);
                    Engine.Created ca = altEngine.applyCreation(alt, a, refs);
                    List<Id> planIds = ids(neuEngine, neu, n, refs);
                    Engine.Created cb = neuEngine.applyCreationAt(neu, n, refs, planIds,
                            names.forStep(neu, neuRules, ri));
                    assertEquals(ids(ca.nodes), ids(cb.nodes), wo + ": erzeugte Knoten");
                    assertEquals(ids(ca.edges), ids(cb.edges), wo + ": erzeugte Verbindungen");
                    assertEquals(abzug(alt, vals), abzug(neu, vals), wo);
                    anwendungen++;
                }
            }
        }
        System.out.println("ApplyCreationTest: anwendungen=" + anwendungen + " arten=" + arten
                + " knoten=" + alt.nodeCount());
        for (String art : List.of("corrFullMatch", "dynamisch", "dynamisch ohne Quelle",
                "Eltern fehlt", "Konstante", "Ghost", "abgeleitet")) {
            assertTrue(arten.getOrDefault(art, 0) > 0, "keine Anwendung der Art " + art);
        }
    }

    /** Zählt die Arten der Plan-Knoten von {@code r} für den Match
     *  {@code refs}, bevor er angewandt wird. Ein Knoten, dessen
     *  erzeugter Eltern-Knoten nicht entsteht, zählt als "Eltern fehlt". */
    private static void arten(Map<String, Integer> arten, Graph g, Rule r, Id[] refs) {
        boolean[] fehlt = new boolean[r.createNodes.size()];
        for (int planIx = 0; planIx < fehlt.length; planIx++) {
            CreateNode cn = r.createNodes.get(planIx);
            String art;
            if (cn.parent < 0 && fehlt[-cn.parent - 1]) {
                art = "Eltern fehlt";
                fehlt[planIx] = true;
            } else if (cn.corrFullMatch) {
                art = "corrFullMatch";
            } else if (cn.dynAttr != null) {
                fehlt[planIx] = g.childLeafOfType(refs[cn.dynAnchor], cn.dynAttr) == null;
                art = fehlt[planIx] ? "dynamisch ohne Quelle" : "dynamisch";
            } else if (cn.konst != null) {
                art = "Konstante";
            } else if (cn.derivedLeaf < 0) {
                art = "Ghost";
            } else {
                art = "abgeleitet";
            }
            arten.merge(art, 1, Integer::sum);
        }
    }
}
