package net.sandrakessler.seesaw.rules;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

/**
 * Ladefehler mit Fundstelle, gespiegelt aus
 * {@code seesaw-core/src/rules/validate.rs}.
 *
 * <p>Für jeden der sechzehn Fälle aus {@link LoadException.Kind} ein
 * Test, der Fehlerklasse UND Fundstelle prüft. Ein Loader, der zwar
 * ablehnt, aber nicht sagt wo, gilt als unfertig.
 */
class ValidateTest {

    /** Rahmen um eine Regel: Kopf plus die eine Regel. */
    private static String file(String rule) {
        return "{\"format\":3,\"name\":\"t\",\"rules\":[" + rule + "]}";
    }

    /** Eine Regel, die durchgeht; Bausteine werden darin ersetzt. */
    private static final String OK_RULE =
            "{\"name\":\"R\",\"rank\":10,"
            + "\"left\":{\"anchor\":\"l0\",\"nodes\":["
            + "{\"name\":\"l0\",\"type\":\"A\"},{\"name\":\"l1\",\"type\":\"aName\"}],"
            + "\"links\":[[\"l0\",\"l1\"]]},"
            + "\"right\":{\"anchor\":\"r0\",\"nodes\":["
            + "{\"name\":\"r0\",\"type\":\"B\"},{\"name\":\"r1\",\"type\":\"bName\"}],"
            + "\"links\":[[\"r0\",\"r1\"]]},"
            + "\"corrs\":[{\"type\":\"C\",\"left\":\"l0\",\"right\":\"r0\","
            + "\"role\":\"establishes\","
            + "\"bindings\":[{\"left\":\"l1\",\"right\":\"r1\"}]}]}";

    private static Validate.Resolved load(String json) {
        return Validate.validate(Format.fromJson(json));
    }

    private static LoadException reject(String json) {
        return assertThrows(LoadException.class, () -> load(json));
    }

    @Test
    void dieBeispielregelGehtDurch() {
        Validate.Resolved r = load(file(OK_RULE));
        assertNotNull(r);
        assertEquals(1, r.rules.size());
        assertEquals("R", r.rules.get(0).name);
    }

    // ── Die sechzehn Faelle ──

    @Test
    void version() {
        LoadException e = reject("{\"format\":2,\"name\":\"t\",\"rules\":[]}");
        assertEquals(LoadException.Kind.VERSION, e.kind);
        assertEquals("unsupported format version 2, expected 3", e.getMessage());
    }

    @Test
    void duplicateRuleName() {
        LoadException e = reject("{\"format\":3,\"name\":\"t\",\"rules\":["
                + OK_RULE + "," + OK_RULE + "]}");
        assertEquals(LoadException.Kind.DUPLICATE_RULE_NAME, e.kind);
        assertEquals("R", e.name);
    }

    @Test
    void duplicateNode() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"l1\",\"type\":\"aName\"}",
                "{\"name\":\"l0\",\"type\":\"aName\"}")));
        assertEquals(LoadException.Kind.DUPLICATE_NODE, e.kind);
        assertEquals("R", e.rule);
        assertEquals("left", e.side);
        assertEquals("l0", e.name);
    }

    @Test
    void duplicateLink() {
        LoadException e = reject(file(OK_RULE.replace(
                "\"links\":[[\"l0\",\"l1\"]]", "\"links\":[[\"l0\",\"l1\"],[\"l0\",\"l1\"]]")));
        assertEquals(LoadException.Kind.DUPLICATE_LINK, e.kind);
        assertEquals("R", e.rule);
        assertEquals("left", e.side);
    }

    @Test
    void duplicateSameValueLink() {
        LoadException e = reject(file(OK_RULE.replace(
                "\"links\":[[\"l0\",\"l1\"]]",
                "\"links\":[[\"l0\",\"l1\"]],"
                + "\"same_value_links\":[[\"l1\",\"l1\"],[\"l1\",\"l1\"]]")));
        assertEquals(LoadException.Kind.DUPLICATE_SAME_VALUE_LINK, e.kind);
        assertEquals("left", e.side);
    }

    @Test
    void unknownNode() {
        LoadException e = reject(file(OK_RULE.replace(
                "\"links\":[[\"l0\",\"l1\"]]", "\"links\":[[\"l0\",\"gibtsNicht\"]]")));
        assertEquals(LoadException.Kind.UNKNOWN_NODE, e.kind);
        assertEquals("R", e.rule);
        assertEquals("left", e.side);
        assertEquals("gibtsNicht", e.name);
    }

    @Test
    void unknownAnchor() {
        LoadException e = reject(file(OK_RULE.replace(
                "\"anchor\":\"l0\"", "\"anchor\":\"gibtsNicht\"")));
        assertEquals(LoadException.Kind.UNKNOWN_ANCHOR, e.kind);
        assertEquals("left", e.side);
        assertEquals("gibtsNicht", e.name);
    }

    @Test
    void sameAsOnLeft() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"l1\",\"type\":\"aName\"}",
                "{\"name\":\"l1\",\"type\":\"aName\",\"same_as\":\"l0\"}")));
        assertEquals(LoadException.Kind.SAME_AS_ON_LEFT, e.kind);
        assertEquals("left", e.side);
        assertEquals("l1", e.name);
    }

    @Test
    void unknownSameAs() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"r1\",\"type\":\"bName\"}",
                "{\"name\":\"r1\",\"type\":\"bName\",\"same_as\":\"gibtsNicht\"}")));
        assertEquals(LoadException.Kind.UNKNOWN_SAME_AS, e.kind);
        assertEquals("gibtsNicht", e.name);
    }

    @Test
    void ambiguousBinding() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"left\":\"l1\",\"right\":\"r1\"}",
                "{\"left\":\"l1\",\"left_type\":\"aName\",\"right\":\"r1\"}")));
        assertEquals(LoadException.Kind.AMBIGUOUS_BINDING, e.kind);
        assertEquals("C", e.name);
    }

    @Test
    void emptyBinding() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"left\":\"l1\",\"right\":\"r1\"}", "{\"right\":\"r1\"}")));
        assertEquals(LoadException.Kind.EMPTY_BINDING, e.kind);
        assertEquals("C", e.name);
    }

    @Test
    void mixedBinding() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"left\":\"l1\",\"right\":\"r1\"}",
                "{\"left\":\"l1\",\"right_type\":\"bName\"}")));
        assertEquals(LoadException.Kind.MIXED_BINDING, e.kind);
        assertEquals("C", e.name);
    }

    @Test
    void predicate() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"l1\",\"type\":\"aName\"}",
                "{\"name\":\"l1\",\"type\":\"aName\","
                + "\"predicate\":{\"kind\":\"regex\",\"pattern\":\"^a$\"}}")));
        assertEquals(LoadException.Kind.PREDICATE, e.kind);
        assertEquals("R", e.rule);
        assertEquals("l1", e.name);
    }

    @Test
    void predicateOnCreatedNode() {
        // r1 wird vorwaerts erzeugt; eine Wertbedingung dort wuerde nie
        // gelesen.
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"r1\",\"type\":\"bName\"}",
                "{\"name\":\"r1\",\"type\":\"bName\","
                + "\"predicate\":{\"kind\":\"exists\"}}")));
        assertEquals(LoadException.Kind.PREDICATE_ON_CREATED_NODE, e.kind);
        assertEquals("right", e.side);
        assertEquals("r1", e.name);
    }

    @Test
    void constantPredicateMismatch() {
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"r1\",\"type\":\"bName\"}",
                "{\"name\":\"r1\",\"type\":\"bName\","
                + "\"predicate\":{\"kind\":\"equals\",\"value\":\"x\"},"
                + "\"constant\":\"y\"}")));
        assertEquals(LoadException.Kind.CONSTANT_PREDICATE_MISMATCH, e.kind);
        assertEquals("right", e.side);
        assertEquals("r1", e.name);
    }

    @Test
    void constantOnMatchedNode() {
        // Ein Kontextknoten wird in KEINER Richtung erzeugt; eine
        // Konstante dort faellt beidseitig durch. (Der Anker l0 taugt
        // dafuer nicht: ihn erzeugt die Rueckrichtung, dort ist eine
        // Konstante zulaessig.)
        LoadException e = reject(file(OK_RULE.replace(
                "{\"name\":\"l1\",\"type\":\"aName\"}",
                "{\"name\":\"l1\",\"type\":\"aName\","
                + "\"context\":true,\"constant\":\"x\"}")));
        assertEquals(LoadException.Kind.CONSTANT_ON_MATCHED_NODE, e.kind);
        assertEquals("left", e.side);
        assertEquals("l1", e.name);
    }

    @Test
    void malformed() {
        LoadException e = reject(file(OK_RULE.replace(
                "\"rank\":10", "\"rank\":10,\"tippfehler\":1")));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
        assertEquals("R", e.rule);
    }

    // ── Was zulaessig ist ──

    @Test
    void gleichheitPlusPasenderKonstanteIstErlaubt() {
        // Die einzige Form, die auf einem erzeugten Knoten zulaessig
        // ist: Gleichheit und Konstante mit demselben Wert.
        Validate.Resolved r = load(file(OK_RULE.replace(
                "{\"name\":\"r1\",\"type\":\"bName\"}",
                "{\"name\":\"r1\",\"type\":\"bName\","
                + "\"predicate\":{\"kind\":\"equals\",\"value\":\"x\"},"
                + "\"constant\":\"x\"}")));
        assertNotNull(r);
    }

    @Test
    void dynamischeBindungAufBeidenSeitenIstErlaubt() {
        Validate.Resolved r = load(file(OK_RULE.replace(
                "{\"left\":\"l1\",\"right\":\"r1\"}",
                "{\"left_type\":\"aName\",\"right_type\":\"bName\"}")));
        assertEquals(1, r.rules.get(0).corrs.get(0).bindings.size());
        assertEquals("aName", r.rules.get(0).corrs.get(0).bindings.get(0).left.leafType);
    }

    @Test
    void ketten_werden_interniert() {
        // Zwei Regeln mit derselben Kette teilen eine Kennung.
        String zwei = "{\"format\":3,\"name\":\"t\",\"rules\":["
                + OK_RULE + ","
                + OK_RULE.replace("\"name\":\"R\"", "\"name\":\"R2\"") + "]}";
        Validate.Resolved r = load(zwei);
        assertEquals(r.rules.get(0).corrs.get(0).bindings.get(0).chain,
                r.rules.get(1).corrs.get(0).bindings.get(0).chain);
    }
}
