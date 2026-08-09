package net.sandrakessler.seesaw.rules;

import java.util.Arrays;
import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.Json;
import net.sandrakessler.seesaw.rules.LoadException;
import net.sandrakessler.seesaw.rules.Prim;
import net.sandrakessler.seesaw.rules.PrimOp;
import net.sandrakessler.seesaw.rules.Transform;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import org.junit.jupiter.api.Test;

import java.util.List;


import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

/**
 * Ketten aus dem Format, gespiegelt aus
 * {@code seesaw-core/src/rules/transform.rs}.
 *
 * <p>Die Prüffälle der Kettenmechanik sind eins zu eins die
 * Rust-Testfälle derselben Datei. Dazu kommen die Fälle, die es in
 * Rust nicht braucht, weil serde sie abfängt: unbekannte Vokabel,
 * fehlendes Argument, überzähliges Feld.
 */
class TransformTest {
    private static final ObjectMapper M = new ObjectMapper();

    private static Chain chainOf(String json) throws Exception {
        JsonNode n = M.readTree(json);
        return Transform.readChain(n, Json.At.none());
    }

    // ── Kettenmechanik: Spiegel der Rust-Tests ──

    @Test
    void getterKetteWirktInListenreihenfolge() throws Exception {
        Chain c = chainOf("[{\"op\":\"capitalize\"},{\"op\":\"prefix\",\"arg\":\"get\"}]");
        assertEquals("getName", c.apply("name"));
    }

    @Test
    void inverseDrehtDieKetteUm() throws Exception {
        Chain c = chainOf("[{\"op\":\"capitalize\"},{\"op\":\"prefix\",\"arg\":\"get\"}]");
        assertEquals("name", c.inverse().apply("getName"));
    }

    @Test
    void praefixOhneTrefferScheitert() throws Exception {
        Chain c = chainOf("[{\"op\":\"prefix\",\"arg\":\"get\"}]");
        assertNull(c.inverse().apply("setName"));
    }

    @Test
    void invertCheckedLiefertQuelleBeiTrefferSonstNull() throws Exception {
        Chain c = chainOf("[{\"op\":\"capitalize\"},{\"op\":\"prefix\",\"arg\":\"get\"}]");
        assertEquals("name", c.invertChecked("getName"));
        assertNull(c.invertChecked("setName"));
    }

    @Test
    void invertCheckedVerwirftUnerreichbareZielwerte() throws Exception {
        Chain c = chainOf("[{\"op\":\"capitalize\"}]");
        // Rückwärts ergibt "a" (decapitalize von "a"), vorwärts daraus
        // aber "A" — "a" ist als Zielwert dieser Kette nicht erreichbar.
        assertEquals("a", c.inverse().apply("a"));
        assertEquals("A", c.apply("a"));
        assertNull(c.invertChecked("a"));
    }

    @Test
    void invertCheckedIstKonsistentNichtOriginalTreu() throws Exception {
        Chain c = chainOf("[{\"op\":\"capitalize\"}]");
        // capitalize("URL") == "URL"; rückwärts kommt "uRL", und das ist
        // richtig, weil es vorwärts wieder auf "URL" abbildet.
        assertEquals("uRL", c.invertChecked("URL"));
        assertEquals("URL", c.apply("uRL"));
    }

    // ── Normalform ──

    /**
     * Die Normalform ändert die Wirkung nicht, für eine Stichprobe.
     *
     * <p>Der Vergleichswert entsteht aus der ROHEN Liste, Schritt für
     * Schritt über {@link Prim#apply}, nicht über eine Kette — jede
     * Kette wäre schon normalisiert und der Vergleich damit leer.
     */
    private static void gleicheWirkung(List<Prim> raw) {
        Chain norm = Chain.chain(raw);
        for (String in : new String[] {"", "x", "ab", "getName", "C cmd", "ÄÖÜ", "ß"}) {
            String roh = in;
            for (Prim p : raw) {
                if (roh == null) break;
                roh = p.apply(roh);
            }
            assertEquals(roh, norm.apply(in),
                    "Normalform ändert die Wirkung auf " + in);
        }
    }

    @Test
    void normalformStreichtWirkungsloseSchritte() {
        List<Prim> raw = List.of(
                new Prim(PrimOp.IDENTITY),
                new Prim(PrimOp.PREFIX, ""),
                new Prim(PrimOp.CAPITALIZE),
                new Prim(PrimOp.SUFFIX, ""),
                new Prim(PrimOp.STRIP_PREFIX, ""),
                new Prim(PrimOp.STRIP_SUFFIX, ""),
                new Prim(PrimOp.IDENTITY));
        assertEquals(List.of(new Prim(PrimOp.CAPITALIZE)), Chain.chain(raw).prims);
        gleicheWirkung(raw);
    }

    @Test
    void normalformZiehtBenachbarteAffixeZusammen() {
        // Listenreihenfolge: "a" kommt zuerst dran, dann "b" davor ⇒ "ba".
        List<Prim> pre = List.of(
                new Prim(PrimOp.PREFIX, "a"), new Prim(PrimOp.PREFIX, "b"));
        assertEquals(List.of(new Prim(PrimOp.PREFIX, "ba")), Chain.chain(pre).prims);
        gleicheWirkung(pre);

        List<Prim> suf = List.of(
                new Prim(PrimOp.SUFFIX, "a"), new Prim(PrimOp.SUFFIX, "b"));
        assertEquals(List.of(new Prim(PrimOp.SUFFIX, "ab")), Chain.chain(suf).prims);
        gleicheWirkung(suf);

        List<Prim> sp = List.of(
                new Prim(PrimOp.STRIP_PREFIX, "a"), new Prim(PrimOp.STRIP_PREFIX, "b"));
        assertEquals(List.of(new Prim(PrimOp.STRIP_PREFIX, "ab")), Chain.chain(sp).prims);

        List<Prim> ss = List.of(
                new Prim(PrimOp.STRIP_SUFFIX, "a"), new Prim(PrimOp.STRIP_SUFFIX, "b"));
        assertEquals(List.of(new Prim(PrimOp.STRIP_SUFFIX, "ba")), Chain.chain(ss).prims);
    }

    @Test
    void normalformLaesstUngleichartigeNachbarnStehen() throws Exception {
        List<Prim> raw = List.of(
                new Prim(PrimOp.PREFIX, "a"),
                new Prim(PrimOp.CAPITALIZE),
                new Prim(PrimOp.PREFIX, "b"));
        assertEquals(raw, Chain.chain(raw).prims);
        gleicheWirkung(raw);

        // Und die beiden Reihenfolgen bleiben unterscheidbar.
        Chain x = chainOf("[{\"op\":\"capitalize\"},{\"op\":\"prefix\",\"arg\":\"get\"}]");
        Chain y = chainOf("[{\"op\":\"prefix\",\"arg\":\"get\"},{\"op\":\"capitalize\"}]");
        assertNotEquals(
                java.util.Arrays.toString(x.identBytes()),
                java.util.Arrays.toString(y.identBytes()));
    }

    @Test
    void bedeutungsgleicheKettenTeilenDieIdentitaet() throws Exception {
        Chain leer = chainOf("[]");
        Chain mitIdentity = chainOf("[{\"op\":\"identity\"},{\"op\":\"identity\"}]");
        Chain leeresPraefix = chainOf("[{\"op\":\"prefix\",\"arg\":\"\"}]");
        assertArrayEquals(leer.identBytes(), mitIdentity.identBytes());
        assertArrayEquals(leer.identBytes(), leeresPraefix.identBytes());

        Chain zwei = chainOf("[{\"op\":\"prefix\",\"arg\":\"a\"},{\"op\":\"prefix\",\"arg\":\"b\"}]");
        Chain eins = chainOf("[{\"op\":\"prefix\",\"arg\":\"ba\"}]");
        assertArrayEquals(zwei.identBytes(), eins.identBytes());
        assertEquals(eins, zwei);
    }

    @Test
    void identBytesUnterscheidenArgumente() throws Exception {
        Chain a = chainOf("[{\"op\":\"prefix\",\"arg\":\"get\"}]");
        Chain b = chainOf("[{\"op\":\"prefix\",\"arg\":\"set\"}]");
        assertNotEquals(
                java.util.Arrays.toString(a.identBytes()),
                java.util.Arrays.toString(b.identBytes()));
    }

    // ── Strenge: was serde in Rust abfängt ──

    @Test
    void fehlendeTransformIstIdentitaet() {
        assertEquals(Chain.IDENTITY, Transform.readChain(null, Json.At.none()));
    }

    @Test
    void stripPraefixIstAusDemFormatNichtSchreibbar() {
        LoadException e = assertThrows(LoadException.class,
                () -> chainOf("[{\"op\":\"strip_prefix\",\"arg\":\"get\"}]"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void unbekanntesPrimitivWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> chainOf("[{\"op\":\"getter_name\"}]"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void praefixOhneArgumentWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> chainOf("[{\"op\":\"prefix\"}]"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void argumentAnArgumentloserArtWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> chainOf("[{\"op\":\"capitalize\",\"arg\":\"x\"}]"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void ueberzaehligesFeldWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> chainOf("[{\"op\":\"prefix\",\"arg\":\"a\",\"tippfehler\":1}]"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }
}
