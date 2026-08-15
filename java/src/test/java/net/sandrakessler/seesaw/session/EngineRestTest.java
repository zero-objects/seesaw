package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;

import org.junit.jupiter.api.Test;

/**
 * Etappe-6-Rest: Konst-Blätter, Retraction/Consolidate — Spiegel der
 * Rust-Tests (rule.rs::konst_tests, engine.rs). Regeln hand-gebaut in
 * der Form, die lower() emittiert (Anker→Corr→Erzeugtes).
 */
class EngineRestTest {

    /** Variante wie Init_Numbering: erzeugt Annotation + Konst-Blatt. */
    private Rule variantRule(String name, long rank, Graph g, String corrTyp, String strategy) {
        Rule r = new Rule();
        r.name = name;
        r.rank = rank;
        r.patNodes = List.of(new PatNode(g.intern("SelectStatement"), null));
        r.patLinks = List.of();
        r.createNodes = new ArrayList<>();
        r.createNodes.add(new CreateNode(corrTyp, 0, -1, null));            // Corr am Anker
        r.createNodes.add(new CreateNode("Annotation", -1, -1, null));      // parent = Corr
        r.createNodes.add(new CreateNode("strategy", -2, -1, null, strategy)); // Konst
        r.createLinks = new ArrayList<>();
        r.createLinks.add(new int[] { 0, -1 });   // Anker → Corr
        r.createLinks.add(new int[] { -2, -3 });  // Annotation → strategy
        r.createLinks.add(new int[] { -1, -2 });  // Corr → Wurzel
        r.inputTypes = List.of("SelectStatement", corrTyp);
        r.corrRecognition.add(new Object[] { corrTyp, 0, "Annotation" });
        return r;
    }

    @Test
    void konstVariantenKollidierenNicht() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        g.addBaseline("sel1", "SelectStatement");
        List<Rule> rules = List.of(
                variantRule("Num_Sequential", 900, g, "NumCorr_seq", "sequential"),
                variantRule("Num_Hierarchical", 890, g, "NumCorr_hier", "hierarchical"));
        Engine e = new Engine(rules);
        e.runToSaturation(g, vals);
        assertEquals(2, e.cascadeLen, "beide Varianten müssen feuern");
        Integer st = g.lookup("strategy");
        TreeSet<String> values = new TreeSet<>();
        for (Id id : g.nodesOfType(st)) values.add(g.resolveValue(id, vals));
        assertEquals(new TreeSet<>(List.of("hierarchical", "sequential")), values);
    }

    @Test
    void retractionUndConsolidateTombstonenErzeugtes() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        Id anchor = g.addBaseline("sel1", "SelectStatement");
        List<Rule> rules =
                List.of(variantRule("Num_Sequential", 900, g, "NumCorr_seq", "sequential"));
        Engine e = new Engine(rules);
        e.runToSaturation(g, vals);
        assertEquals(1, e.cascadeLen);
        // Baseline fällt weg → Provenienz-Walk → Erzeugtes TT → Tombstone.
        g.setNodeStatus(anchor, St.TOMBSTONE);
        e.elementDeleted(g, anchor);
        e.consolidate(g);
        Integer at = g.lookup("Annotation");
        int tombstoned = 0;
        for (Graph.Slot s : g.map.values()) {
            if (s.node != null && s.node.typ == at && s.node.status == St.TOMBSTONE)
                tombstoned++;
        }
        assertEquals(1, tombstoned, "erzeugte Annotation muss Tombstone sein");
        assertTrue(g.nodesOfType(at).isEmpty(), "nicht mehr matchbar");
    }
}
