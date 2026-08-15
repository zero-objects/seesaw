package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.Test;

/**
 * E3: {@code Graph.materialize()} gegen die Rust-Referenz — Spiegel
 * des Rust-Tests {@code fold_materialisiert_ohne_tombstones}
 * (engine/mod.rs): gleiche Szene (F2P-Forward, 2 Familien, Member 0
 * tombstonen + retract + consolidate), gleiche vier Zusicherungen.
 */
class FoldMaterializeTest {

    private static void seedFwd(int n, Graph g, Map<Id, String> vals) {
        for (int i = 0; i < n; i++) {
            Id f = g.addBaseline("f" + i, "Family");
            Id r = g.addBaseline("f" + i + "/father", "Father");
            Id m = g.addBaseline("f" + i + "/father/m", "Member");
            Id leaf = g.addBaseline("f" + i + "/father/m/fn", "firstName");
            g.connect(f, r, St.SOLID);
            g.connect(r, m, St.SOLID);
            g.connect(m, leaf, St.SOLID);
            vals.put(leaf, "John" + i);
        }
    }

    @Test
    void foldMaterialisiertOhneTombstones() throws Exception {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(2, g, vals);
        List<Rule> all = Fixtures.rules("f2p", g);
        Engine e = new Engine(List.of(all.get(0))); // nur fwd
        e.run(g, vals, 100);

        Id member = e.cascade.get(0).refs[2];
        g.setNodeStatus(member, St.TOMBSTONE);
        e.elementDeleted(g, member);
        e.consolidate(g);

        Graph folded = g.materialize();
        assertNull(folded.node(member), "Tombstone fällt weg");
        for (Id c : e.cascade.get(0).created) {
            assertNull(folded.node(c), "retractete Erzeugnisse fallen weg");
        }
        for (Id c : e.cascade.get(1).created) {
            assertEquals(St.SOLID, folded.node(c).status, "Ghost→Solid");
        }
        // Abgeleitetes Blatt behält Provenienz — Wert weiter auflösbar.
        Id nameLeaf = e.cascade.get(1).created.get(2);
        assertNotNull(folded.resolveValue(nameLeaf, vals));
    }

    @Test
    void consolidateZaehltEliminierteElemente() throws Exception {
        // Additiv (E3): consolidate liefert die Zahl finalisierter TT
        // (Knoten + Kanten) — das eliminated-Feld des fold-Reports.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        seedFwd(1, g, vals);
        List<Rule> all = Fixtures.rules("f2p", g);
        Engine e = new Engine(List.of(all.get(0)));
        e.run(g, vals, 100);
        int created = e.cascade.get(0).created.size();
        int createdEdges = e.cascade.get(0).createdEdges.size();

        Id member = e.cascade.get(0).refs[2];
        g.setNodeStatus(member, St.TOMBSTONE);
        e.elementDeleted(g, member);
        assertEquals(created + createdEdges, e.consolidate(g),
            "alle Erzeugnisse des retracteten Matches finalisiert");
        assertEquals(0, e.consolidate(g), "zweite Konsolidierung leer");
    }
}
