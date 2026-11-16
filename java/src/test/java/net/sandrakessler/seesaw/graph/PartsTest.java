package net.sandrakessler.seesaw.graph;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.Collections;
import java.util.ConcurrentModificationException;
import java.util.HexFormat;
import java.util.Iterator;
import java.util.List;
import java.util.Random;
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

    // ── Parts selbst, gegen eine TreeSet ──

    /** Pseudozufällige Id: die ersten vier Byte aus i, wie im Rust-Test. */
    private static Id id(int i) {
        byte[] b = new byte[32];
        int v = i * (int) 2_654_435_761L;
        b[0] = (byte) (v >>> 24);
        b[1] = (byte) (v >>> 16);
        b[2] = (byte) (v >>> 8);
        b[3] = (byte) v;
        return new Id(b);
    }

    /** Die i-te Participation: drei Gegentypen, je zwei mit gleichem Gegenende. */
    private static Part part(int i) {
        return new Part(i % 3, id(i / 2), i % 2 == 0, id(1000 + i));
    }

    /** Parts und TreeSet antworten gleich: Durchlauf, Größe, Typbereiche, Enthaltensein. */
    private static void gleich(TreeSet<Part> erwartet, Parts parts, String wo) {
        assertEquals(liste(erwartet), liste(parts), "Durchlauf " + wo);
        assertEquals(erwartet.size(), parts.size(), "size " + wo);
        assertEquals(erwartet.isEmpty(), parts.isEmpty(), "isEmpty " + wo);
        for (int typ : TYPEN) {
            Iterable<Part> sicht = parts.ofType(typ);
            List<String> alt = liste(bisherig(erwartet, typ));
            assertEquals(alt, liste(sicht), "ofType " + typ + " " + wo);
            assertEquals(alt, liste(sicht), "ofType zweiter Lauf " + typ + " " + wo);
        }
        for (Part p : erwartet) assertTrue(parts.contains(p), "contains " + wo);
    }

    @Test
    void partsGleichDerTreeSetBeimEinfuegen() {
        for (int n : GROESSEN) {
            Parts parts = new Parts();
            TreeSet<Part> erwartet = new TreeSet<>();
            for (int i = 0; i < n; i++) {
                Part p = part(i);
                assertTrue(parts.add(p), "neu " + i);
                erwartet.add(p);
                // Ein zweites Mal: die Liste ist eine Menge.
                assertFalse(parts.add(p), "doppelt " + i);
                assertFalse(parts.add(new Part(p.otherTyp, p.other, p.outgoing, p.connection)), "gleich " + i);
                gleich(erwartet, parts, "n=" + n + " nach " + (i + 1));
            }
            assertFalse(parts.contains(part(n)), "fehlt n=" + n);
            // Bis 48 Einträge ein Array, ab dem 49. ein Baum (Rust: v.len() >= PARTS_TREE_AT).
            assertEquals(n > Parts.TREE_AT, parts.isTree(), "Form n=" + n);
        }
    }

    @Test
    void entfernenUeberDieGrenzeHaeltFolgeUndInhalt() {
        for (int n : GROESSEN) {
            Parts parts = new Parts();
            TreeSet<Part> erwartet = new TreeSet<>();
            List<Part> alle = new ArrayList<>();
            for (int i = 0; i < n; i++) {
                parts.add(part(i));
                erwartet.add(part(i));
                alle.add(part(i));
            }
            Collections.shuffle(alle, new Random(n));
            // Erst bis auf ein Viertel herunter, dann alles wieder hinein, dann ganz leer.
            int bis = n / 4;
            for (int k = 0; k < n - bis; k++) {
                Part p = alle.get(k);
                assertTrue(parts.remove(p), "entfernt n=" + n + " k=" + k);
                assertFalse(parts.remove(p), "schon entfernt n=" + n + " k=" + k);
                erwartet.remove(p);
                assertFalse(parts.contains(p));
                gleich(erwartet, parts, "n=" + n + " nach Entfernen " + (k + 1));
            }
            for (int k = n - bis - 1; k >= 0; k--) {
                assertTrue(parts.add(alle.get(k)));
                erwartet.add(alle.get(k));
                gleich(erwartet, parts, "n=" + n + " wieder " + k);
            }
            for (int k = 0; k < n; k++) {
                assertTrue(parts.remove(alle.get(k)));
                erwartet.remove(alle.get(k));
                gleich(erwartet, parts, "n=" + n + " leeren " + k);
            }
            assertTrue(parts.isEmpty());
            assertFalse(parts.iterator().hasNext());
        }
    }

    @Test
    void aendernWaehrendDesDurchlaufsWirdGemeldet() {
        for (int n : new int[] {3, 60}) {
            Parts parts = new Parts();
            for (int i = 0; i < n; i++) parts.add(part(i));
            Iterator<Part> alle = parts.iterator();
            Iterator<Part> typ0 = parts.ofType(0).iterator();
            alle.next();
            typ0.next();
            parts.add(part(n));
            assertThrows(ConcurrentModificationException.class, alle::next, "iterator n=" + n);
            assertThrows(ConcurrentModificationException.class, typ0::next, "ofType n=" + n);
        }
    }

    // ── Slot und connectReporting ──

    @Test
    void verbindungsplatzOhneListeKnotenplatzMitListe() {
        Graph g = new Graph();
        Id s = g.addBaseline("s", "A");
        Id t = g.addBaseline("t", "B");
        Id c = g.connect(s, t, St.SOLID);
        assertNull(g.map.get(c).parts, "Platz der Verbindung ohne Liste");
        assertNotNull(g.map.get(c).connection);
        assertNull(g.map.get(c).node);
        assertNotNull(g.map.get(s).parts);
        assertNotNull(g.map.get(t).parts);
        // Ein leerer Platz aus slot() hat wie bisher eine Liste.
        assertNotNull(g.slot(Ident.identBaseline("neu")).parts);
        assertNotNull(new Graph.Slot().parts);
        // Ein Hub über 48 Participations hält sie in Baumform.
        Graph h = new Graph();
        Id hub = hub(h, 200, new TreeSet<>());
        assertTrue(h.map.get(hub).parts.isTree());
    }

    @Test
    void schleifeTraegtBeideRichtungen() {
        Graph g = new Graph();
        Id s = g.addBaseline("s", "A");
        Id c = g.connect(s, s, St.SOLID);
        int a = g.node(s).typ;
        TreeSet<Part> erwartet = new TreeSet<>();
        erwartet.add(new Part(a, s, true, c));
        erwartet.add(new Part(a, s, false, c));
        assertEquals(liste(erwartet), liste(g.parts(s)));
        assertFalse(g.connectReporting(s, s, St.SOLID).fresh);
        assertEquals(liste(erwartet), liste(g.parts(s)));
    }

    @Test
    void connectReportingWieBisher() {
        Graph g = new Graph();
        Id s = g.addBaseline("s", "A");
        Id t = g.addBaseline("t", "B");
        g.setNodeStatus(s, St.TENTATIVE_TOMBSTONE);
        g.setNodeStatus(t, St.TENTATIVE_TOMBSTONE);
        // Eine tentativ zurückgezogene Verbindung belebt die Endpunkte nicht.
        Graph.ConnResult r = g.connectReporting(s, t, St.TENTATIVE_TOMBSTONE);
        assertTrue(r.fresh);
        assertEquals(Ident.identConnection(s, t), r.id);
        assertEquals(St.TENTATIVE_TOMBSTONE, g.node(s).status);
        assertEquals(St.TENTATIVE_TOMBSTONE, g.node(t).status);
        // Eine lebendige belebt beide Endpunkte (M5) und die Verbindung.
        r = g.connectReporting(s, t, St.GHOST);
        assertFalse(r.fresh);
        assertEquals(St.GHOST, g.node(s).status);
        assertEquals(St.GHOST, g.node(t).status);
        assertEquals(St.GHOST, g.conn(r.id).status);
        // Eine bestehende lebendige Verbindung behält ihren Status.
        r = g.connectReporting(s, t, St.SOLID);
        assertFalse(r.fresh);
        assertEquals(St.GHOST, g.conn(r.id).status);
        // Tombstone als Endpunkt oder fehlender Endpunkt: keine Verbindung.
        Id u = g.addBaseline("u", "C");
        g.setNodeStatus(u, St.TOMBSTONE);
        assertNull(g.connectReporting(s, u, St.SOLID));
        assertNull(g.connectReporting(u, s, St.SOLID));
        assertNull(g.connectReporting(s, Ident.identBaseline("fehlt"), St.SOLID));
        assertNull(g.connectReporting(Ident.identBaseline("fehlt"), s, St.SOLID));
        // Je Seite eine Participation, am Tombstone keine.
        assertEquals(1, liste(g.parts(s)).size());
        assertEquals(1, liste(g.parts(t)).size());
        assertEquals(0, liste(g.parts(u)).size());
    }
}
