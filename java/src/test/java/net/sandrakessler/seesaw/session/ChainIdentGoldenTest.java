package net.sandrakessler.seesaw.session;
import net.sandrakessler.seesaw.rules.PrimOp;
import net.sandrakessler.seesaw.rules.Prim;

import java.util.List;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.Transform;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.HashMap;
import java.util.Map;

import org.junit.jupiter.api.Test;

/**
 * Gleichlauf-Gate für die Ketten-Identität: dieselbe Kette, dieselbe
 * GhostId in Rust und Java. Die Golden-Werte stammen aus
 * {@code cargo test -p seesaw-core --test v2_chain_ident_golden --
 * --ignored} und sind nicht von Hand geschrieben.
 *
 * <p>Die Ketten im Golden sind ROH notiert (nicht in Normalform) —
 * die Java-Seite muss dieselbe Normalisierung anwenden wie
 * {@code Chain::normalized}, sonst laufen die im Block {@code gleich}
 * gepaarten Fälle auseinander.
 */
class ChainIdentGoldenTest {

    private static String hex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    private static JsonNode golden() throws Exception {
        return Fixtures.resource("/fixtures/chain_ident_golden.json");
    }

    @Test
    void ketten_identitaet_wie_rust() throws Exception {
        JsonNode g = golden();
        Id parent = Ident.identBaseline(g.get("parent_external").asText());
        Id source = Ident.identBaseline(g.get("source_external").asText());
        assertEquals(g.get("parent").asText(), hex(parent), "Anker-Id (V2B)");
        assertEquals(g.get("source").asText(), hex(source), "Quell-Id (V2B)");
        String typ = g.get("typ").asText();
        for (JsonNode c : g.get("cases")) {
            Chain t = Fixtures.transform(c.get("transform"));
            assertEquals(c.get("id").asText(), hex(Ident.identDerived(parent, typ, source, t)),
                    c.get("name").asText());
        }
    }

    @Test
    void normalform_faelle_teilen_die_identitaet() throws Exception {
        JsonNode g = golden();
        Id parent = Ident.identBaseline(g.get("parent_external").asText());
        Id source = Ident.identBaseline(g.get("source_external").asText());
        String typ = g.get("typ").asText();
        Map<String, String> ids = new HashMap<>();
        for (JsonNode c : g.get("cases")) {
            Chain t = Fixtures.transform(c.get("transform"));
            ids.put(c.get("name").asText(), hex(Ident.identDerived(parent, typ, source, t)));
        }
        for (JsonNode pair : g.get("gleich")) {
            String a = pair.get(0).asText();
            String b = pair.get(1).asText();
            assertEquals(ids.get(a), ids.get(b), a + " vs " + b);
        }
        assertNotEquals(ids.get("capitalize_dann_praefix"), ids.get("praefix_dann_capitalize"),
                "Reihenfolge bleibt unterscheidbar");
    }

    /**
     * Die zusammengesetzten Ketten, die früher als benanntes Vokabular
     * im Kern standen, müssen in beiden Sprachen dieselbe Identität
     * ergeben. Die Namen sind aus dem Kern verschwunden, die Ketten
     * gibt es weiter — geprüft über die Identität, für alle elf.
     */
    @Test
    void bestandsvokabular_wie_transform_plan() throws Exception {
        JsonNode g = golden();
        Id parent = Ident.identBaseline(g.get("parent_external").asText());
        Id source = Ident.identBaseline(g.get("source_external").asText());
        String typ = g.get("typ").asText();
        int n = 0;
        for (JsonNode v : g.get("vokabular")) {
            String name = v.get("name").asText();
            // Kette aus dem Golden und Java-Zuordnung müssen beide passen.
            assertEquals(v.get("id").asText(),
                    hex(Ident.identDerived(parent, typ, source,
                            Fixtures.transform(v.get("transform")))),
                    name + " (Kette aus dem Golden)");
            n++;
        }
        assertEquals(11, n, "alle Ketten im Golden");
    }

    /** Wert-Semantik der Kette (strikt, Spiegel von {@code Chain::apply}). */
    @Test
    void kette_wendet_in_listenreihenfolge_an() {
        Chain getter = Chain.chain(List.of(
                new Prim(PrimOp.CAPITALIZE), new Prim(PrimOp.PREFIX, "get")));
        Chain getterStrip = Chain.chain(List.of(
                new Prim(PrimOp.STRIP_PREFIX, "get"), new Prim(PrimOp.DECAPITALIZE)));
        Chain cmd = Chain.chain(List.of(new Prim(PrimOp.PREFIX, "C ")));
        assertEquals("getName", getter.apply("name"));
        assertEquals("name", getterStrip.apply("getName"));
        assertEquals("C cmd", cmd.apply("cmd"));
    }
}
