package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/** Gegenbeispiele fuer die wellenfeste Delta-Richtung, gespiegelt zu Rust. */
final class DirectionalWaveTest {

    private static void saturate(Engine e, Graph g, Map<Id, String> vals) {
        while (e.step(g, vals) != null) { /* bis zum Fixpunkt */ }
    }

    private static String fatherRule() {
        return """
            {"format":3,"name":"direction_test","rules":[{
              "name":"Father_2_Male","rank":850,
              "left":{"anchor":"fam","nodes":[
                {"name":"fam","type":"Family"},
                {"name":"father","type":"Father"},
                {"name":"member","type":"Member"},
                {"name":"first","type":"firstName"}],
                "links":[["fam","father"],["father","member"],["member","first"]]},
              "right":{"anchor":"male","nodes":[
                {"name":"male","type":"Male"},{"name":"name","type":"name"}],
                "links":[["male","name"]]},
              "corrs":[{"type":"PersonCorr","left":"member","right":"male",
                "role":"establishes","bindings":[{"left":"first","right":"name"}]}]
            }]}""";
    }

    private static String followUpRules() {
        return """
            {"format":3,"name":"follow_up","rules":[
              {"name":"Root","rank":100,
               "left":{"anchor":"a","nodes":[{"name":"a","type":"A"}],"links":[]},
               "right":{"anchor":"b","nodes":[{"name":"b","type":"B"}],"links":[]},
               "corrs":[{"type":"AB","left":"a","right":"b","role":"establishes"}]},
              {"name":"Child","rank":90,
               "left":{"anchor":"a","nodes":[{"name":"a","type":"A"},
                       {"name":"x","type":"X"}],"links":[["a","x"]]},
               "right":{"anchor":"b","nodes":[{"name":"b","type":"B"},
                        {"name":"y","type":"Y"}],"links":[["b","y"]]},
               "corrs":[{"type":"AB","left":"a","right":"b","role":"references"},
                        {"type":"XY","left":"x","right":"y","role":"establishes"}]}
            ]}""";
    }

    private static Id sourceFamily(Graph g, Map<Id, String> vals) {
        Id fam = g.addBaseline("f", "Family");
        Id father = g.addBaseline("f/father", "Father");
        Id member = g.addBaseline("f/father/member", "Member");
        Id first = g.addBaseline("f/father/member/first", "firstName");
        g.connect(fam, father, St.SOLID);
        g.connect(father, member, St.SOLID);
        g.connect(member, first, St.SOLID);
        vals.put(first, "John");
        return first;
    }

    @Test
    void sourceDeltaRecordsAndExecutesOnlyForwardPlans() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(fatherRule(), g);
        Map<Id, String> vals = new HashMap<>();
        sourceFamily(g, vals);
        Engine e = new Engine(rules);

        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, vals);
        assertTrue(e.matches.stream()
                .allMatch(m -> rules.get(m.ruleIx).direction == Rule.Direction.FORWARD));
        saturate(e, g, vals);
        assertEquals(1, e.cascadeLen);
        assertTrue(e.cascade.stream()
                .allMatch(entry -> rules.get(entry.ruleIx).direction == Rule.Direction.FORWARD));
    }

    @Test
    void explicitDomainDisambiguatesEqualAnchorTypeNames() {
        String json = """
            {"format":3,"name":"same_metamodel","rules":[{
              "name":"A_2_A","rank":10,
              "left":{"anchor":"l","nodes":[{"name":"l","type":"A"}],"links":[]},
              "right":{"anchor":"r","nodes":[{"name":"r","type":"A"}],"links":[]},
              "corrs":[{"type":"AA","left":"l","right":"r","role":"establishes"}]
            }]}""";
        Graph g = new Graph();
        List<Rule> rules = Rules.load(json, g);
        g.addBaseline("source/a", "A");
        Engine e = new Engine(rules);

        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, Map.of());
        assertTrue(e.matches.stream()
                .allMatch(m -> rules.get(m.ruleIx).direction == Rule.Direction.FORWARD));
    }

    @Test
    void newForwardCorrespondenceEnablesForwardFollowUpInSameWave() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(followUpRules(), g);
        Id a = g.addBaseline("a", "A");
        Id x = g.addBaseline("a/x", "X");
        g.connect(a, x, St.SOLID);
        Engine e = new Engine(rules);

        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, Map.of());
        saturate(e, g, Map.of());

        assertEquals(List.of("Root→", "Child→"), e.cascade.stream()
                .map(entry -> rules.get(entry.ruleIx).name).toList());
        assertEquals(1, g.nodesOfType(g.lookup("Y")).stream()
                .filter(id -> g.node(id).status.matchable()).count());
    }

    @Test
    void attributeChangeUsesAdmittedDomainInsteadOfLeafType() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(fatherRule(), g);
        Map<Id, String> vals = new HashMap<>();
        Id sourceName = sourceFamily(g, vals);
        Engine e = new Engine(rules);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.run(g, vals, 100);
        e.consolidate(g);

        vals.put(sourceName, "Renamed");
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.elementChanged(g, sourceName);
        e.seed(g, vals);
        saturate(e, g, vals);
        e.consolidate(g);

        Id targetName = g.nodesOfType(g.lookup("name")).stream()
                .filter(id -> g.node(id).status.matchable()).findFirst().orElseThrow();
        assertEquals("Renamed", g.resolveValue(targetName, vals));
    }

    @Test
    void attributeLeafAddEntersThroughTheAddDoor() {
        Graph g = new Graph();
        Id family = g.addBaseline("f", "Family");
        Id father = g.addBaseline("f/father", "Father");
        Id member = g.addBaseline("f/father/member", "Member");
        g.connect(family, father, St.SOLID);
        g.connect(father, member, St.SOLID);
        List<Rule> rules = Rules.load(fatherRule(), g);
        Map<Id, String> vals = new HashMap<>();
        Engine e = new Engine(rules);

        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, vals);
        assertTrue(e.step(g, vals) == null, "ohne Blatt kein Match");

        Id leaf = g.addBaseline("f/father/member/first", "firstName");
        g.connect(member, leaf, St.SOLID);
        vals.put(leaf, "John");
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.elementsAdded(g, vals, List.of(leaf));
        saturate(e, g, vals);

        Id targetName = g.nodesOfType(g.lookup("name")).stream()
                .filter(id -> g.node(id).status.matchable()).findFirst().orElseThrow();
        assertEquals("John", g.resolveValue(targetName, vals));
        assertTrue(e.cascade.stream()
                .allMatch(entry -> rules.get(entry.ruleIx).direction == Rule.Direction.FORWARD));
    }

    @Test
    void attributeLeafDeleteEntersThroughTheDeleteDoor() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(fatherRule(), g);
        Map<Id, String> vals = new HashMap<>();
        Id sourceName = sourceFamily(g, vals);
        Engine e = new Engine(rules);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.run(g, vals, 100);
        e.consolidate(g);

        Id targetName = g.nodesOfType(g.lookup("name")).stream()
                .filter(id -> g.node(id).status.matchable()).findFirst().orElseThrow();
        g.setNodeStatus(sourceName, St.TOMBSTONE);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.elementDeleted(g, sourceName);
        e.consolidate(g);

        assertEquals(St.TOMBSTONE, g.node(targetName).status,
                "the attribute correspondence carries the delete to the target leaf");
        assertEquals(1, g.nodesOfType(g.lookup("Member")).stream()
                .filter(id -> g.node(id).status.matchable()).count(),
                "the externally supplied source carrier survives");
        assertEquals(0, g.nodesOfType(g.lookup("Male")).stream()
                .filter(id -> g.node(id).status.matchable()).count(),
                "firstName is a match prerequisite, so its deletion invalidates the complete "
                        + "Father_2_Male realisation");
    }

    @Test
    void linkAddEntersThroughTheAddDoor() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(followUpRules(), g);
        Id a = g.addBaseline("a", "A");
        Id x = g.addBaseline("a/x", "X");
        Engine e = new Engine(rules);

        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.seed(g, Map.of());
        saturate(e, g, Map.of());
        assertEquals(List.of("Root→"), e.cascade.stream()
                .map(entry -> rules.get(entry.ruleIx).name).toList());

        g.connect(a, x, St.SOLID);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.linkAdded(g, Map.of(), a, x);
        saturate(e, g, Map.of());

        assertEquals(List.of("Root→", "Child→"), e.cascade.stream()
                .map(entry -> rules.get(entry.ruleIx).name).toList());
    }

    @Test
    void linkDeleteEntersThroughTheDeleteDoor() {
        Graph g = new Graph();
        List<Rule> rules = Rules.load(followUpRules(), g);
        Id a = g.addBaseline("a", "A");
        Id x = g.addBaseline("a/x", "X");
        Id edge = g.connect(a, x, St.SOLID);
        Engine e = new Engine(rules);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.run(g, Map.of(), 100);
        e.consolidate(g);

        Id y = g.nodesOfType(g.lookup("Y")).stream()
                .filter(id -> g.node(id).status.matchable()).findFirst().orElseThrow();
        g.setConnectionStatus(edge, St.TOMBSTONE);
        e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
        e.linkRemoved(g, a, x);
        e.seed(g, Map.of());
        saturate(e, g, Map.of());
        e.consolidate(g);

        assertEquals(St.TOMBSTONE, g.node(y).status,
                "removing the source link invalidates the Child realisation");
        assertTrue(g.node(a).status.matchable());
        assertTrue(g.node(x).status.matchable());
    }
}
