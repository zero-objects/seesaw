package net.sandrakessler.seesaw.session;
import net.sandrakessler.seesaw.rules.PrimOp;
import net.sandrakessler.seesaw.rules.Prim;
import java.util.List;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.rules.Chain;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

/**
 * Direktes bit-exakt-Gate für die Identitäts-Ableitung
 * (V2B/V2G/V2D/V2C/V2R/V2K, blake3). Die Erwartungswerte kommen aus
 * {@code /fixtures/ident_golden.json}, das der Rust-Test
 * {@code schreibt_ident_golden} mit denselben festen Eingaben
 * erzeugt. Bis 2026-08-10 standen sie hartkodiert hier, mit dem
 * Vermerk sie stammten aus Rust -- ohne Erzeuger, der das belegt. Divergenz der
 * Byte-Serialisierung schlägt hier direkt an — nicht erst über die
 * Anwendungs-Zahlen der Äquivalenz-Tests.
 */
class GhostIdGoldenTest {

    /** Die Kette, die frueher `getter_name` hiess. */
    private static Chain getterChain() {
        return Chain.chain(List.of(new Prim(PrimOp.CAPITALIZE), new Prim(PrimOp.PREFIX, "get")));
    }

    private static final Id CLASS = Ident.identBaseline("uml:/Person");
    private static final Id NAME = Ident.identBaseline("uml:/Person/name");

    /** Ein Wert aus dem von Rust erzeugten Golden. */
    private static String golden(String key) {
        try {
            return Fixtures.resource("/fixtures/ident_golden.json").get(key).asText();
        } catch (Exception e) {
            throw new IllegalStateException("Ident-Golden fehlt: " + key, e);
        }
    }

    private static String hex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    @Test
    void baselineV2B() {
        assertEquals(
                golden("baseline_class"),
                hex(CLASS));
        assertEquals(
                golden("baseline_name"),
                hex(NAME));
    }

    @Test
    void ghostV2G() {
        assertEquals(
                golden("ghost"),
                hex(Ident.identGhost(CLASS, "Member")));
    }

    /**
     * Die Transformation geht als KETTE ein (Task 5c). Der Wert war
     * bis dahin 5f032802617f09b56b73a585a204ec291ee42809dfdad604b16d23b02bc2923e
     * — das Tag-Byte 2 des früheren Enums (`GetterName`). Neu ist es
     * die Byte-Folge von `[Capitalize, Prefix("get")]`; der alte Wert
     * gehört jetzt zu `CompSymbolInv` (Tag-Byte 2 von PlanTransform).
     * Golden aus Rust: {@code /fixtures/chain_ident_golden.json},
     * Fall {@code capitalize_dann_praefix}.
     */
    @Test
    void derivedV2D() {
        assertEquals(
                golden("derived"),
                hex(Ident.identDerived(CLASS, "getterName", NAME, getterChain())));
    }

    @Test
    void connectionV2C() {
        assertEquals(
                golden("connection"),
                hex(Ident.identConnection(CLASS, NAME)));
    }

    @Test
    void corrV2R() {
        assertEquals(
                golden("corr"),
                hex(Ident.identCorr(CLASS, "Corr", new Id[] {CLASS, NAME})));
    }

    @Test
    void konstV2K() {
        assertEquals(
                golden("konst"),
                hex(Ident.identKonst(CLASS, "Op", "mkOp", 3)));
    }
}
