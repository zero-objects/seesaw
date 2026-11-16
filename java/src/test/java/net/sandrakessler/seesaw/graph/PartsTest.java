package net.sandrakessler.seesaw.graph;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.HexFormat;
import java.util.List;
import java.util.TreeSet;

import org.junit.jupiter.api.Test;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;

/**
 * Participation-Liste eines Knotens: Typbereich und Reihenfolge wie die
 * bisherige Fassung über einer {@code TreeSet} (Java 2.0.2).
 */
class PartsTest {

    /** Zahl der Participations des Hubs, um den Wechsel bei 48 herum. */
    private static final int[] GROESSEN = {1, 47, 48, 49, 200};

    /** Gegentypen, nach denen gefragt wird: vorhandene, fehlende und die Ränder von int. */
    private static final int[] TYPEN = {Integer.MIN_VALUE, -1, 0, 1, 2, 3, 4, 5, Integer.MAX_VALUE};

    /** {@code partsByOtherType} in der Fassung von Java 2.0.2. */
    static List<Part> bisherig(TreeSet<Part> parts, int typ) {
        List<Part> out = new ArrayList<>();
        Part from = new Part(typ, new Id(new byte[32]), false, new Id(new byte[32]));
        for (Part p : parts.tailSet(from, true)) {
            if (p.otherTyp != typ) break;
            out.add(p);
        }
        return out;
    }

    /** Part hat kein equals: verglichen werden die vier Felder als Text, in der Folge des Durchlaufs. */
    static List<String> liste(Iterable<Part> it) {
        List<String> out = new ArrayList<>();
        for (Part p : it) {
            out.add(p.otherTyp + " " + HexFormat.of().formatHex(p.other.b) + " " + p.outgoing
                    + " " + HexFormat.of().formatHex(p.connection.b));
        }
        return out;
    }

    /**
     * Ein Hub mit {@code n} Nachbarn über drei Typen (A, B, C). Der Hub
     * hat den Typ 0, A bis C die Typen 1 bis 3. Gerade Nachbarn hängen
     * am Hub als Ziel, ungerade als Quelle, jeder fünfte in beide
     * Richtungen. {@code erwartet} bekommt die Participations des Hubs.
     */
    private static Id hub(Graph g, int n, TreeSet<Part> erwartet) {
        Id hub = g.addBaseline("hub", "Hub");
        String[] typen = {"A", "B", "C"};
        for (int i = 0; i < n; i++) {
            Id x = g.addBaseline("x" + i, typen[i % 3]);
            int t = g.node(x).typ;
            if (i % 2 == 0 || i % 5 == 0) {
                assertNotNull(g.connect(hub, x, St.SOLID));
                erwartet.add(new Part(t, x, true, Ident.identConnection(hub, x)));
            }
            if (i % 2 == 1 || i % 5 == 0) {
                assertNotNull(g.connect(x, hub, St.SOLID));
                erwartet.add(new Part(t, x, false, Ident.identConnection(x, hub)));
            }
        }
        return hub;
    }

    @Test
    void hubLiefertJeTypDieBisherigeFolge() {
        for (int n : GROESSEN) {
            Graph g = new Graph();
            TreeSet<Part> erwartet = new TreeSet<>();
            Id h = hub(g, n, erwartet);
            assertEquals(liste(erwartet), liste(g.parts(h)), "parts n=" + n);
            for (int typ : TYPEN) {
                List<String> alt = liste(bisherig(erwartet, typ));
                Iterable<Part> sicht = g.partsOfType(h, typ);
                assertEquals(alt, liste(sicht), "partsOfType n=" + n + " typ=" + typ);
                // Die Sicht lässt sich mehrmals durchlaufen.
                assertEquals(alt, liste(sicht), "partsOfType zweiter Lauf n=" + n + " typ=" + typ);
                assertEquals(alt, liste(g.partsByOtherType(h, typ)), "partsByOtherType n=" + n + " typ=" + typ);
            }
            int summe = 0;
            for (int typ = 1; typ <= 3; typ++) summe += liste(g.partsOfType(h, typ)).size();
            assertEquals(erwartet.size(), summe, "drei Typen decken die Liste n=" + n);
        }
    }

    @Test
    void nachbarnSehenDenHub() {
        Graph g = new Graph();
        TreeSet<Part> amHub = new TreeSet<>();
        Id h = hub(g, 49, amHub);
        for (int i = 0; i < 49; i++) {
            Id x = Ident.identBaseline("x" + i);
            TreeSet<Part> erwartet = new TreeSet<>();
            if (i % 2 == 0 || i % 5 == 0) erwartet.add(new Part(0, h, false, Ident.identConnection(h, x)));
            if (i % 2 == 1 || i % 5 == 0) erwartet.add(new Part(0, h, true, Ident.identConnection(x, h)));
            assertEquals(liste(erwartet), liste(g.parts(x)), "Nachbar " + i);
            for (int typ : TYPEN) {
                assertEquals(liste(bisherig(erwartet, typ)), liste(g.partsOfType(x, typ)), "Nachbar " + i + " typ=" + typ);
            }
        }
    }

    @Test
    void partsByOtherTypeLiefertEineEigeneListe() {
        Graph g = new Graph();
        TreeSet<Part> erwartet = new TreeSet<>();
        Id h = hub(g, 10, erwartet);
        List<Part> a = g.partsByOtherType(h, 1);
        assertTrue(a instanceof ArrayList, "ArrayList wie bisher");
        int vorher = a.size();
        a.clear();
        a.add(null);
        assertEquals(vorher, g.partsByOtherType(h, 1).size(), "die Liste des Graphen bleibt");
        assertEquals(liste(erwartet), liste(g.parts(h)));
    }

    @Test
    void verbindungUndFehlendeIdOhneParticipations() {
        Graph g = new Graph();
        Id s = g.addBaseline("s", "A");
        Id t = g.addBaseline("t", "B");
        Id c = g.connect(s, t, St.SOLID);
        assertNotNull(g.conn(c));
        for (int typ : TYPEN) {
            assertFalse(g.parts(c).iterator().hasNext(), "parts(Verbindung)");
            assertFalse(g.partsOfType(c, typ).iterator().hasNext(), "partsOfType(Verbindung)");
            assertTrue(g.partsByOtherType(c, typ).isEmpty(), "partsByOtherType(Verbindung)");
        }
        Id fehlt = Ident.identBaseline("fehlt");
        assertFalse(g.parts(fehlt).iterator().hasNext());
        assertFalse(g.partsOfType(fehlt, 0).iterator().hasNext());
        assertTrue(g.partsByOtherType(fehlt, 0).isEmpty());
    }
}
