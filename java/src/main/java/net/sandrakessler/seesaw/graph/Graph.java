package net.sandrakessler.seesaw.graph;

import net.sandrakessler.seesaw.ident.Ident;

import net.sandrakessler.seesaw.rules.Chain;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import net.sandrakessler.seesaw.engine.Matcher;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.rules.Transform;

public final class Graph {
    public final HashMap<Id, Slot> map = new HashMap<>();
    public final LinkedHashMap<String, Integer> typesByName = new LinkedHashMap<>();
    final ArrayList<String> typeNames = new ArrayList<>();
    final HashMap<Integer, ArrayList<Id>> byType = new HashMap<>();
    /** Regel-Konstanten (Quellmaterial-Tabelle, §3b.4). */
    final LinkedHashMap<String, Integer> konstByValue = new LinkedHashMap<>();
    final ArrayList<String> konstValues = new ArrayList<>();

    public int internKonst(String value) {
        Integer k = konstByValue.get(value);
        if (k != null) return k;
        int ix = konstValues.size();
        konstValues.add(value);
        konstByValue.put(value, ix);
        return ix;
    }

    public int intern(String name) {
        Integer t = typesByName.get(name);
        if (t != null) return t;
        int id = typeNames.size();
        typeNames.add(name);
        typesByName.put(name, id);
        return id;
    }

    public Integer lookup(String name) { return typesByName.get(name); }

    public String typeName(int t) { return typeNames.get(t); }

    public Slot slot(Id id) { return map.computeIfAbsent(id, k -> new Slot()); }

    public Node node(Id id) {
        Slot s = map.get(id);
        return s == null ? null : s.node;
    }

    public Conn conn(Id id) {
        Slot s = map.get(id);
        return s == null ? null : s.connection;
    }

    public void insertNode(Node n) {
        // Ein neuer Platz kommt mit seiner Liste aus slot().
        Slot s = slot(n.id);
        if (s.node != null) {
            if (s.node.status == St.TENTATIVE_TOMBSTONE) s.node.status = n.status;
            return;
        }
        s.node = n;
        partsOf(s);
        byType.computeIfAbsent(n.typ, k -> new ArrayList<>()).add(n.id);
    }

    /** Die Liste im Platz eines Knotens. Ein Platz ohne Liste (er hielt
     *  bisher nur eine Verbindung) bekommt hier eine. */
    private static Parts partsOf(Slot s) {
        Parts p = s.parts;
        if (p == null) s.parts = p = new Parts();
        return p;
    }

    public Id addBaseline(String external, String typName) {
        Id id = Ident.identBaseline(external);
        insertNode(new Node(id, intern(typName), St.SOLID, null, null));
        return id;
    }

    public Id addGhost(Id parent, String typName) {
        Id id = Ident.identGhost(parent, typName);
        insertNode(new Node(id, intern(typName), St.GHOST, null, null));
        return id;
    }

    public Id addDerivedLeaf(Id parent, String typName, Id source, Chain t) {
        Id id = Ident.identDerived(parent, typName, source, t);
        insertNode(new Node(id, intern(typName), St.GHOST, source, t));
        return id;
    }

    /** Corr-Knoten (Match-Digest-Identität, Spec §1.4). */
    /**
     * Markiert einen vorhandenen Knoten als Korrespondenz. Getrennt vom
     * Anlegen, weil eine Korrespondenz im kanonischen Regelformat ueber
     * dieselben Wege entsteht wie jeder andere Knoten -- die Markierung
     * darf die Identitaet nicht beruehren.
     */
    public void markCorr(Id id) {
        Node n = node(id);
        if (n != null) n.isCorr = true;
    }

    public Id addCorr(Id anchor, String typName, Id[] refs) {
        Id id = Ident.identCorr(anchor, typName, refs);
        insertNode(new Node(id, intern(typName), St.GHOST, null, null));
        return id;
    }

    /** Erstes matchbares Kind-Blatt vom Typ (dynamische Bindings). */
    public Id childLeafOfType(Id anchor, String typName) {
        Integer t = typesByName.get(typName);
        if (t == null) return null;
        for (Part p : partsOfType(anchor, t)) {
            if (!p.outgoing) continue;
            Node n = node(p.other);
            if (n != null && n.status.matchable()) return p.other;
        }
        return null;
    }

    /** Regel-Konstanten-Blatt (§3b.4): Wert wohnt im Regelsatz. */
    public Id addKonstLeaf(Id parent, String typName, String ruleName, int planIx, String value) {
        Id id = Ident.identKonst(parent, typName, ruleName, planIx);
        insertNode(new Node(id, intern(typName), St.GHOST, null, null, internKonst(value)));
        return id;
    }

    public void setNodeStatus(Id id, St st) {
        Node n = node(id);
        if (n != null) n.status = st;
    }

    /** Ergebnis von connectReporting: Kanten-Id + ob NEU angelegt. */
    public static final class ConnResult {
        public final Id id;
        public final boolean fresh;

        ConnResult(Id id, boolean fresh) {
            this.id = id;
            this.fresh = fresh;
        }
    }

    public Id connect(Id source, Id target, St status) {
        ConnResult r = connectReporting(source, target, status);
        return r == null ? null : r.id;
    }

    /**
     * Spiegel von connect_reporting: M5-Reklamation durch Nutzung + fresh-Flag.
     * Jeder Endpunkt wird einmal nachgeschlagen, sein Platz liefert Knoten
     * und Liste. Der Platz der Verbindung wird einmal nachgeschlagen und
     * trägt keine Liste.
     */
    public ConnResult connectReporting(Id source, Id target, St status) {
        Slot ss = map.get(source), ts = map.get(target);
        Node s = ss == null ? null : ss.node, t = ts == null ? null : ts.node;
        if (s == null || t == null) return null;
        if (s.status == St.TOMBSTONE || t.status == St.TOMBSTONE) return null;
        // Reklamation durch Nutzung (M5): lebendige Verbindung
        // re-legitimiert einen tentativ zurückgezogenen Endpunkt.
        if (status != St.TOMBSTONE && status != St.TENTATIVE_TOMBSTONE) {
            if (s.status == St.TENTATIVE_TOMBSTONE) s.status = St.GHOST;
            if (t.status == St.TENTATIVE_TOMBSTONE) t.status = St.GHOST;
        }
        Id id = Ident.identConnection(source, target);
        Slot cs = map.computeIfAbsent(id, Graph::connectionSlot);
        if (cs.connection != null) {
            if (cs.connection.status == St.TENTATIVE_TOMBSTONE)
                cs.connection.status = status;
            return new ConnResult(id, false);
        }
        cs.connection = new Conn(id, source, target, status);
        partsOf(ss).add(new Part(t.typ, target, true, id));
        partsOf(ts).add(new Part(s.typ, source, false, id));
        return new ConnResult(id, true);
    }

    /** Neuer Platz für eine Verbindung, ohne Liste. */
    private static Slot connectionSlot(Id id) {
        return new Slot(null);
    }

    public void setConnectionStatus(Id id, St st) {
        Conn c = conn(id);
        if (c != null) c.status = st;
    }

    /** matchbar (Solid|Ghost|TT) — für den Matcher. */
    public boolean connected(Id s, Id t) {
        Conn c = conn(Ident.identConnection(s, t));
        return c != null && c.status.matchable();
    }

    /** nicht-tentativ (Solid|Ghost) — für die Duplikat-Unterdrückung. */
    public boolean connectedAlive(Id s, Id t) {
        Conn c = conn(Ident.identConnection(s, t));
        return c != null && c.status.alive();
    }

    public Iterable<Part> parts(Id id) {
        Slot s = map.get(id);
        return s == null || s.parts == null ? List.of() : s.parts;
    }

    /** Range über (otherTyp, *) wie parts_by_other_type, als eigene
     *  Liste (der Aufrufer darf sie verändern). */
    public List<Part> partsByOtherType(Id id, int typ) {
        Slot s = map.get(id);
        if (s == null) return List.of();
        List<Part> out = new ArrayList<>();
        if (s.parts != null) for (Part p : s.parts.ofType(typ)) out.add(p);
        return out;
    }

    /**
     * Die Participations von {@code id}, deren Gegenende den Typ
     * {@code typ} hat, in Part-Ordnung (Rust {@code Graph::parts_by_other_type}
     * über {@code Parts::of_type}). Eine Sicht auf die Liste des Knotens:
     * ein Aufruf legt weder eine Liste noch eine {@code Id} an. Wer die
     * Liste während des Durchlaufs ändert, bekommt eine
     * {@code ConcurrentModificationException}.
     */
    public Iterable<Part> partsOfType(Id id, int typ) {
        Slot s = map.get(id);
        return s == null || s.parts == null ? List.of() : s.parts.ofType(typ);
    }

    /** Zahl der Knoten eines Typs, gleich welchen Status. */
    public int typeCount(int typ) {
        ArrayList<Id> ids = byType.get(typ);
        return ids == null ? 0 : ids.size();
    }

    /** Zahl aller Knoten, gleich welchen Status. */
    public int nodeCount() {
        int n = 0;
        for (ArrayList<Id> ids : byType.values()) n += ids.size();
        return n;
    }

    public List<Id> nodesOfType(int typ) {
        ArrayList<Id> out = new ArrayList<>();
        for (Id id : byType.getOrDefault(typ, new ArrayList<>())) {
            Node n = node(id);
            if (n != null && n.status.matchable()) out.add(id);
        }
        return out;
    }

    // ── E2-Session-Zugriff (additiv, für session) ──

    /** Alle Knoten der Map (inkl. Blätter) — Snapshot-Iteration. */
    public List<Node> allNodes() {
        ArrayList<Node> out = new ArrayList<>();
        for (Slot s : map.values()) if (s.node != null) out.add(s.node);
        return out;
    }

    /** Alle Verbindungen der Map — Snapshot-Iteration. */
    public List<Conn> allConnections() {
        ArrayList<Conn> out = new ArrayList<>();
        for (Slot s : map.values()) if (s.connection != null) out.add(s.connection);
        return out;
    }

    /** Baseline-Knoten unter EXTERN aufgelöster Id (Session-Opaque-
     *  Bridge: registerExternalOpaque liefert Ids, die nicht über
     *  Ident.identBaseline(opaque) herleitbar sind). Idempotent. */
    public void insertExternal(Id id, String typName) {
        insertNode(new Node(id, intern(typName), St.SOLID, null, null));
    }

    /** Materialisierung (Fold-Endstufe, Spiegel von Rust
     *  {@code Graph::materialize}): neuer Graph ohne Tombstones,
     *  alle Ghosts werden Solid; Verbindungen nur, wenn beide
     *  Endpunkte überleben. Abgeleitete Blätter behalten
     *  Quelle+Transform, Konstanten ihre Tabelle — Werte löst
     *  weiterhin der Resolver auf. */
    public Graph materialize() {
        Graph out = new Graph();
        out.typeNames.addAll(typeNames);
        out.typesByName.putAll(typesByName);
        out.konstValues.addAll(konstValues);
        out.konstByValue.putAll(konstByValue);
        for (Slot s : map.values()) {
            Node n = s.node;
            if (n == null || n.status == St.TOMBSTONE) continue;
            Node m = new Node(n.id, n.typ, St.SOLID,
                    n.derivedSource, n.derivedTransform, n.konstIx);
            // Die Korrespondenz-Eigenschaft ueberlebt die
            // Materialisierung: das Loeschen laeuft an ihr entlang, und
            // ein materialisiertes Erzeugnis bleibt zurueckziehbar.
            m.isCorr = n.isCorr;
            out.insertNode(m);
        }
        for (Slot s : map.values()) {
            Conn c = s.connection;
            if (c == null || c.status == St.TOMBSTONE) continue;
            if (out.node(c.source) != null && out.node(c.target) != null) {
                out.connect(c.source, c.target, St.SOLID);
            }
        }
        return out;
    }

    public String resolveValue(Id id, Map<Id, String> values) {
        ArrayList<Chain> chain = new ArrayList<>();
        Id cur = id;
        while (true) {
            Node n = node(cur);
            if (n == null) return null;
            if (n.konstIx >= 0) {
                return applyChain(chain, konstValues.get(n.konstIx));
            }
            if (n.derivedSource == null) {
                String v = values.get(cur);
                if (v == null) return null;
                return applyChain(chain, v);
            }
            chain.add(n.derivedTransform);
            cur = n.derivedSource;
        }
    }

    /** Kette von der Quelle her anwenden; null = ein Schritt war
     *  nicht anwendbar (Spiegel des `?` in `resolve_value`). */
    private static String applyChain(List<Chain> chain, String value) {
        String v = value;
        for (int i = chain.size() - 1; i >= 0; i--) {
            v = chain.get(i).apply(v);
            if (v == null) return null;
        }
        return v;
    }

    /**
     * Platz eines Teilnehmers in der Map: ein Knoten mit seiner
     * Participation-Liste oder eine Verbindung (Rust {@code Slot}). Der
     * Platz einer Verbindung trägt keine Liste.
     */
    public static final class Slot {
        public Node node;
        public Conn connection;
        /** Participations des Knotens, in Part-Ordnung. null im Platz
         *  einer Verbindung. */
        public Parts parts;

        /** Leerer Platz mit Liste, wie bisher. */
        public Slot() {
            parts = new Parts();
        }

        /** Platz einer Verbindung, ohne Liste. */
        private Slot(Conn connection) {
            this.connection = connection;
        }
    }
}
