package net.sandrakessler.seesaw.session;
import net.sandrakessler.seesaw.rules.PrimOp;
import net.sandrakessler.seesaw.rules.Prim;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.rules.Chain;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;

import org.junit.jupiter.api.Test;


/**
 * E2 Schritt 1: Δ-JSON → Graph. Op-Semantik gespiegelt an
 * seesaw-jni::apply_json_op, Format aus seesaw.emf.DeltaBuilder.
 */
class DeltaCodecTest {

    /** Die Kette, die frueher `getter_name` hiess. */
    private static Chain getterChain() {
        return Chain.chain(List.of(new Prim(PrimOp.CAPITALIZE), new Prim(PrimOp.PREFIX, "get")));
    }

    private static SessionRules demoMeta() {
        return new SessionRules(
            List.of(),
            Set.of("name", "type"),
            Set.of("CorrClass"),
            Map.of(
                SessionRules.comboKey("Model", "Class"), "classes",
                SessionRules.comboKey("Model", "JavaClass"), "javaClasses"),
            Set.of());
    }

    private static String classDelta() {
        return """
            {"origin":"User","op_star":[
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model","attrs":{}},
              {"type":"AddNode","parent":"m1","childId":"c1",
               "edgeType":"classes","typeId":"Class","attrs":{"name":"Foo"}}
            ]}""";
    }

    @Test
    void addNodeErzeugtKnotenBlattUndVerbindung() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());

        DeltaCodec.Result r = codec.apply(classDelta());

        assertEquals("User", r.origin);
        assertEquals(2, r.received);
        assertEquals(2, r.applied);
        assertTrue(r.errors.isEmpty(), "keine Fehler: " + r.errors);

        Id m = Ident.identBaseline("m1");
        Id c = Ident.identBaseline("c1");
        Id leaf = Ident.identBaseline("c1/name");
        assertEquals("Model", g.typeName(g.node(m).typ));
        assertEquals("Class", g.typeName(g.node(c).typ));
        assertEquals("name", g.typeName(g.node(leaf).typ));
        assertTrue(g.connected(m, c), "Model→Class verbunden");
        assertTrue(g.connected(c, leaf), "Class→name-Blatt verbunden");
        assertEquals("Foo", g.resolveValue(leaf, vals));

        // Routing-Futter: berührte Typen + neue Knoten (inkl. Blatt).
        assertTrue(r.deltaTypes.containsAll(Set.of("Model", "Class", "name")));
        assertTrue(r.newNodes.contains(c));
        assertTrue(r.newNodes.contains(leaf));
    }

    @Test
    void addNodeIstIdempotentFuerExistierendeKnoten() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());

        codec.apply(classDelta());
        DeltaCodec.Result r2 = codec.apply(classDelta());

        assertEquals(2, r2.applied);
        assertTrue(r2.errors.isEmpty());
        assertTrue(r2.newNodes.isEmpty(), "kein Knoten doppelt angelegt");
        // Attrs des existierenden Kindes bleiben unangetastet (JNI-Parität).
        assertEquals("Foo", g.resolveValue(Ident.identBaseline("c1/name"), vals));
    }

    @Test
    void setAttrAktualisiertBaselineBlatt() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"SetAttr","target":"c1","key":"name","value":"Bar"}]}""");

        assertEquals(1, r.applied);
        assertEquals("Bar", g.resolveValue(Ident.identBaseline("c1/name"), vals));
    }

    @Test
    void setAttrLegtFehlendesBlattAn() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        codec.apply("""
            {"origin":"User","op_star":[
              {"type":"SetAttr","target":"c1","key":"visibility","value":"public"}]}""");

        Id leaf = g.childLeafOfType(Ident.identBaseline("c1"), "visibility");
        assertNotNull(leaf, "neues Blatt angelegt");
        assertEquals("public", g.resolveValue(leaf, vals));
    }

    @Test
    void setAttrSchreibtDurchAbgeleitetesBlattZurueck() {
        // Regel-erzeugtes Blatt (GETTER_NAME auf dem Quell-Blatt):
        // SetAttr auf dem abgeleiteten Blatt muss invers (GETTER_STRIP)
        // auf die Quelle durchschreiben — rc7-A8-Parität.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        Id src = Ident.identBaseline("c1/name");
        Id getter = g.addBaseline("g1", "Getter");
        Id derived = g.addDerivedLeaf(getter, "name", src, getterChain());
        g.connect(getter, derived, St.GHOST);
        assertEquals("getFoo", g.resolveValue(derived, vals));

        // Opaque für den Getter registrieren (wie registerExternalOpaque).
        codec.registerOpaque("gen/Getter/g1", getter);
        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"SetAttr","target":"gen/Getter/g1","key":"name","value":"getBar"}]}""");

        assertTrue(r.errors.isEmpty(), "" + r.errors);
        assertEquals("bar", g.resolveValue(src, vals),
            "invers durchgeschrieben (GETTER_STRIP)");
        assertEquals("getBar", g.resolveValue(derived, vals),
            "abgeleitetes Blatt löst den neuen Wert auf");
    }

    @Test
    void setAttrVerwirftInkonsistentenZielwert() {
        // Kette [Capitalize, Prefix("get")], Zielwert "getname":
        // invers ergibt "name", vorwaerts ergibt daraus aber "getName"
        // und nicht "getname". Der Zielwert ist ueber diese Kette gar
        // nicht erreichbar — der Schreibvorgang muss scheitern, sonst
        // loest das Blatt danach auf etwas anderes auf als gefordert.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        Id src = Ident.identBaseline("c1/name");
        Id getter = g.addBaseline("g1", "Getter");
        Id derived = g.addDerivedLeaf(getter, "name", src, getterChain());
        g.connect(getter, derived, St.GHOST);
        codec.registerOpaque("gen/Getter/g1", getter);

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"SetAttr","target":"gen/Getter/g1","key":"name","value":"getname"}]}""");

        assertEquals(1, r.errors.size(), "" + r.errors);
        assertTrue(r.errors.get(0).contains("kein konsistenter Quellwert"), r.errors.get(0));
        assertEquals("Foo", g.resolveValue(src, vals), "Quelle unveraendert");
        assertEquals("getFoo", g.resolveValue(derived, vals), "Blatt unveraendert");

        // Die Konsistenzpruefung direkt, ohne Codec-Umweg.
        assertNull(getterChain().invertChecked("getname"));
        assertEquals("name", getterChain().invertChecked("getName"));
        // Und der Fall "Rueckwaerts-Kette gar nicht anwendbar".
        assertNull(getterChain().invertChecked("setName"));
    }

    @Test
    void delNodeTombstonetUndMeldet() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"c1"}]}""");

        assertEquals(1, r.applied);
        Id c = Ident.identBaseline("c1");
        assertEquals(St.TOMBSTONE, g.node(c).status);
        assertTrue(r.removedNodes.contains(c));
    }

    @Test
    void delNodeTombstonetAuchAttrBlaetter() {
        // E3: Paritaet zur ersten Generation — Attrs wohnen dort IM Knoten und fallen mit
        // ihm; hier sind sie Blatt-Subknoten und werden mit-tombstonet
        // (samt Trage-Verbindung), Blätter erscheinen in removedNodes.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"c1"}]}""");

        Id c = Ident.identBaseline("c1");
        Id leaf = Ident.identBaseline("c1/name");
        assertEquals(St.TOMBSTONE, g.node(leaf).status);
        assertEquals(St.TOMBSTONE,
            g.conn(Ident.identConnection(c, leaf)).status);
        assertTrue(r.removedNodes.containsAll(List.of(c, leaf)));
    }

    @Test
    void delNodeFolgtSolidCorrsZuPartnern() {
        // E4: korrespondenz-folgende Retraction (Spiegel von
        // retraction_cascade_for, rc8) — materialisierte (Solid)
        // Corr-Kette Class → CorrClass → JavaClass fällt beim DelNode
        // der Class komplett, inkl. Partner-Blatt und Verbindungen.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        Id c = Ident.identBaseline("c1");
        Id corr = g.addGhost(c, "CorrClass");
        Id jc = g.addGhost(corr, "JavaClass");
        Id dleaf = g.addDerivedLeaf(jc, "name",
            Ident.identBaseline("c1/name"), Chain.IDENTITY);
        g.connect(c, corr, St.GHOST);
        g.connect(corr, jc, St.GHOST);
        g.connect(jc, dleaf, St.GHOST);
        // Materialisierter Zustand (post-fold): alles Solid.
        for (Id id : List.of(corr, jc, dleaf)) g.setNodeStatus(id, St.SOLID);

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"c1"}]}""");

        assertTrue(r.errors.isEmpty(), "" + r.errors);
        assertEquals(St.TOMBSTONE, g.node(c).status);
        assertEquals(St.TOMBSTONE, g.node(corr).status, "Corr fällt mit");
        assertEquals(St.TOMBSTONE, g.node(jc).status, "Partner fällt mit");
        assertEquals(St.TOMBSTONE, g.node(dleaf).status, "Partner-Blatt fällt mit");
        assertEquals(St.TOMBSTONE,
            g.conn(Ident.identConnection(corr, jc)).status);
        assertTrue(r.removedNodes.containsAll(List.of(c, corr, jc)));
    }

    @Test
    void delNodeFolgtGhostCorrsNicht() {
        // Ghost-Erzeugnisse gehören der Provenienz-Retraction der
        // Engine (TT + Resurrektions-Fenster) — der Codec-Corr-Hop
        // greift nur für Solid (materialisiert).
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        Id c = Ident.identBaseline("c1");
        Id corr = g.addGhost(c, "CorrClass");
        Id jc = g.addGhost(corr, "JavaClass");
        g.connect(c, corr, St.GHOST);
        g.connect(corr, jc, St.GHOST);

        codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"c1"}]}""");

        assertEquals(St.TOMBSTONE, g.node(c).status);
        assertEquals(St.GHOST, g.node(corr).status,
            "Ghost-Corr bleibt der Engine-Provenienz überlassen");
        assertEquals(St.GHOST, g.node(jc).status);
    }

    @Test
    void delNodeAufUnbekanntemZielIstFehler() {
        Graph g = new Graph();
        DeltaCodec codec = new DeltaCodec(g, new HashMap<>(), demoMeta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"nope"}]}""");

        assertEquals(0, r.applied);
        assertEquals(1, r.errors.size());
        assertTrue(r.errors.get(0).contains("DelNode target not found"));
    }

    @Test
    void delEdgeTombstonetVerbindungUndMeldetEndpunkte() {
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"DelEdge","source":"m1","target":"c1","edgeType":"classes"}]}""");

        assertEquals(1, r.applied);
        Id m = Ident.identBaseline("m1");
        Id c = Ident.identBaseline("c1");
        assertEquals(St.TOMBSTONE,
            g.conn(Ident.identConnection(m, c)).status);
        assertEquals(1, r.removedLinks.size());
        assertEquals(m, r.removedLinks.get(0)[0]);
        assertEquals(c, r.removedLinks.get(0)[1]);
    }

    @Test
    void hexOpaqueEinesExistierendenKnotensAdressiertDirekt() {
        // rc8-Forward-Delete-Parität: die 64-Hex-Form einer Knoten-Id
        // (seesawId) löst über attach/detach hinweg auf den realen
        // Knoten auf — kein Phantom.
        Graph g = new Graph();
        Map<Id, String> vals = new HashMap<>();
        DeltaCodec codec = new DeltaCodec(g, vals, demoMeta());
        codec.apply(classDelta());

        Id c = Ident.identBaseline("c1");
        String hexOpaque = SnapshotCodec.hex(c);
        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[{"type":"DelNode","target":"%s"}]}"""
            .formatted(hexOpaque));

        assertTrue(r.errors.isEmpty(), "" + r.errors);
        assertEquals(St.TOMBSTONE, g.node(c).status);
    }

    @Test
    void unbekannterOpTypIstFehlerOhneAbbruch() {
        Graph g = new Graph();
        DeltaCodec codec = new DeltaCodec(g, new HashMap<>(), demoMeta());

        DeltaCodec.Result r = codec.apply("""
            {"origin":"User","op_star":[
              {"type":"Explode"},
              {"type":"AddNode","parent":"root","childId":"m1",
               "edgeType":"contains","typeId":"Model"}]}""");

        assertEquals(1, r.applied);
        assertEquals(1, r.errors.size());
        assertNotNull(g.node(Ident.identBaseline("m1")), "Folge-Op noch angewandt");
        assertNull(g.node(Ident.identBaseline("Explode")));
    }
}
