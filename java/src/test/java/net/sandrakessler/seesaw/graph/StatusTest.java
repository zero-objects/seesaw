package net.sandrakessler.seesaw.graph;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;

/**
 * Status lesen und setzen in einem Zugriff ({@code retractNode},
 * {@code retractConnection}, {@code settleNode}, {@code settleConnection})
 * gegen das Lesen und Setzen in zwei Zugriffen, wie es die Engine bisher
 * tat. Dazu {@code connectionIdTyped} gegen {@code connectionId}.
 */
class StatusTest {

    /** Zwei Knoten, eine Verbindung, alle drei mit Status {@code st}. */
    private static Graph graph(St st) {
        Graph g = new Graph();
        Id s = g.addBaseline("s", "A");
        Id t = g.addBaseline("t", "B");
        Id c = g.connect(s, t, St.SOLID);
        g.setNodeStatus(s, st);
        g.setConnectionStatus(c, st);
        return g;
    }

    private static final Id S = Ident.identBaseline("s");
    private static final Id T = Ident.identBaseline("t");
    private static final Id C = Ident.identConnection(S, T);
    private static final Id FEHLT = Ident.identBaseline("fehlt");

    @Test
    void zuruecknehmenWieLesenUndSetzen() {
        for (St st : St.values()) {
            Graph neu = graph(st);
            Graph alt = graph(st);
            // Wie Engine.retractMatch in 2.0.2.
            Node n = alt.node(S);
            boolean nodeAlt = n != null && n.status.matchable();
            if (nodeAlt) alt.setNodeStatus(S, St.TENTATIVE_TOMBSTONE);
            Conn c = alt.conn(C);
            boolean connAlt = c != null && c.status.matchable();
            if (connAlt) alt.setConnectionStatus(C, St.TENTATIVE_TOMBSTONE);

            assertEquals(nodeAlt, neu.retractNode(S), st.toString());
            assertEquals(connAlt, neu.retractConnection(C), st.toString());
            assertEquals(alt.node(S).status, neu.node(S).status, st.toString());
            assertEquals(alt.conn(C).status, neu.conn(C).status, st.toString());
            // Ohne Platz, oder der Platz haelt das andere: nichts.
            assertFalse(neu.retractNode(FEHLT));
            assertFalse(neu.retractConnection(FEHLT));
            assertFalse(neu.retractNode(C));
            assertFalse(neu.retractConnection(S));
        }
    }

    @Test
    void festschreibenWieLesenUndSetzen() {
        for (St st : St.values()) {
            for (boolean corr : new boolean[] {false, true}) {
                Graph neu = graph(st);
                Graph alt = graph(st);
                if (corr) {
                    neu.markCorr(S);
                    alt.markCorr(S);
                }
                // Wie Engine.consolidate in 2.0.2.
                Node n = alt.node(S);
                boolean nodeAlt = false;
                boolean corrAlt = false;
                if (n != null && n.status == St.TENTATIVE_TOMBSTONE) {
                    alt.setNodeStatus(S, St.TOMBSTONE);
                    nodeAlt = true;
                    corrAlt = n.isCorr;
                }
                Conn c = alt.conn(C);
                boolean connAlt = false;
                if (c != null && c.status == St.TENTATIVE_TOMBSTONE) {
                    alt.setConnectionStatus(C, St.TOMBSTONE);
                    connAlt = true;
                }

                Node settled = neu.settleNode(S);
                String was = st + " corr=" + corr;
                if (nodeAlt) {
                    assertSame(neu.node(S), settled, was);
                    assertEquals(corrAlt, settled.isCorr, was);
                } else {
                    assertNull(settled, was);
                }
                assertEquals(connAlt, neu.settleConnection(C), was);
                assertEquals(alt.node(S).status, neu.node(S).status, was);
                assertEquals(alt.conn(C).status, neu.conn(C).status, was);
                assertNull(neu.settleNode(FEHLT));
                assertFalse(neu.settleConnection(FEHLT));
                assertNull(neu.settleNode(C));
                assertFalse(neu.settleConnection(S));
            }
        }
    }

    @Test
    void verbindungsIdMitTypWieOhne() {
        Graph g = new Graph();
        Id[] ids = {
            g.addBaseline("a1", "A"), g.addBaseline("a2", "A"),
            g.addBaseline("b1", "B"), g.addBaseline("b2", "B"),
        };
        g.connect(ids[0], ids[2], St.SOLID);
        g.connect(ids[2], ids[0], St.SOLID);
        g.connect(ids[1], ids[1], St.SOLID);
        Id tot = g.connect(ids[1], ids[3], St.SOLID);
        g.setConnectionStatus(tot, St.TOMBSTONE);
        int typA = g.lookup("A");
        int typB = g.lookup("B");
        int found = 0;
        for (Id s : ids) {
            for (Id t : ids) {
                Id want = g.connectionId(s, t);
                if (want != null) found++;
                int typ = g.node(t).typ;
                assertEquals(want, g.connectionIdTyped(s, t, typ));
                // Ein anderer Typ als der von t findet nichts.
                assertNull(g.connectionIdTyped(s, t, typ == typA ? typB : typA));
            }
            assertNull(g.connectionIdTyped(s, FEHLT, typA));
            assertNull(g.connectionIdTyped(FEHLT, s, g.node(s).typ));
        }
        assertEquals(4, found);
    }
}
