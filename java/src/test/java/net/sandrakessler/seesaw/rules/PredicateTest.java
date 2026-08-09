package net.sandrakessler.seesaw.rules;

import net.sandrakessler.seesaw.rules.LoadException;
import net.sandrakessler.seesaw.rules.Numbers;
import net.sandrakessler.seesaw.rules.Predicate;
import net.sandrakessler.seesaw.rules.RegexSubset;

import com.fasterxml.jackson.databind.ObjectMapper;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Wert-Prädikate, gespiegelt aus {@code rules/predicate.rs}.
 *
 * <p>Jeder Prüffall der Rust-Datei steht hier wieder, mit denselben
 * Mustern und denselben Erwartungen. Dazu kommen die Fälle, die es nur
 * in Java gibt: die verbotenen Klassen-Kurzformen, und das Lesen aus
 * der Formatdarstellung.
 */
class PredicateTest {
    private static final ObjectMapper M = new ObjectMapper();

    private static Predicate regex(String pattern) {
        return Predicate.parseRegex(pattern, "R", "n");
    }

    private static Predicate read(String json) throws Exception {
        return Predicate.read(M.readTree(json), "R", "n");
    }

    // ── Spiegel der Rust-Testfälle ──

    @Test
    void regexIstVollmatch() {
        Predicate p = regex("ab");
        assertTrue(p.matches("ab"));
        assertFalse(p.matches("xaby"), "ein Teiltreffer darf nicht zutreffen");
    }

    @Test
    void ankerSindVerboten() {
        LoadException e = assertThrows(LoadException.class, () -> regex("^ab$"));
        assertEquals(LoadException.Kind.PREDICATE, e.kind);
    }

    @Test
    void lookaroundUndRueckwaertsverweiseSindVerboten() {
        for (String pat : new String[] {"(?=a)", "(?!a)", "(\\w)\\1", "\\bx", "\\p{L}"}) {
            LoadException e = assertThrows(LoadException.class, () -> regex(pat),
                    pat + " muss abgelehnt werden");
            assertEquals(LoadException.Kind.PREDICATE, e.kind);
        }
    }

    @Test
    void zahlengrammatikIstEng() {
        assertEquals(-1500.0, Numbers.parse("-1.5e3"));
        assertEquals(42.0, Numbers.parse("42"));
        assertNull(Numbers.parse("1d"), "Java-Suffix ist nicht erlaubt");
        assertNull(Numbers.parse("0x1p3"), "Hex-Float ist nicht erlaubt");
        assertNull(Numbers.parse("inf"));
        assertNull(Numbers.parse("NaN"));
        // Zwei Fälle, die nur Java anbietet und die deshalb in Rust
        // nicht vorkommen: Javas Double.parseDouble nimmt beides an.
        assertNull(Numbers.parse("Infinity"));
        assertNull(Numbers.parse(" 42"), "fuehrendes Leerzeichen ist nicht erlaubt");
    }

    @Test
    void zahlenbereichIstInklusive() {
        Predicate p = new Predicate.NumericRange(1.0, 2.0);
        assertTrue(p.matches("1"));
        assertTrue(p.matches("2"));
        assertFalse(p.matches("2.1"));
    }

    @Test
    void praefixUndGleichheitUndVorhandensein() {
        assertTrue(new Predicate.Prefix("cmd_").matches("cmd_go"));
        assertTrue(new Predicate.Equals("x").matches("x"));
        assertTrue(new Predicate.Exists().matches(""));
        assertFalse(new Predicate.Exists().matches(null));
    }

    @Test
    void negierteZeichenklasseIstErlaubt() {
        Predicate p = regex("[^a]");
        assertTrue(p.matches("b"));
        assertFalse(p.matches("a"));
    }

    @Test
    void echterAnkerBleibtVerbotenAuchNebenEinerZeichenklasse() {
        assertThrows(LoadException.class, () -> regex("[a]^"));
    }

    @Test
    void escapteAnkerzeichenSindErlaubt() {
        Predicate p = regex("a\\^b\\$c");
        assertTrue(p.matches("a^b$c"));
    }

    @Test
    void benannteGruppenBeideSchreibweisenSindVerboten() {
        for (String pat : new String[] {"(?P<name>a)", "(?<name>a)"}) {
            assertThrows(LoadException.class, () -> regex(pat),
                    pat + " muss abgelehnt werden");
        }
    }

    @Test
    void possessiveQuantorenAlleVierFormenSindVerboten() {
        for (String pat : new String[] {"a*+", "a++", "a?+", "a{2,3}+"}) {
            assertThrows(LoadException.class, () -> regex(pat),
                    pat + " muss abgelehnt werden");
        }
    }

    @Test
    void escapteLiteraleVorEinfachemQuantorSindErlaubt() {
        Predicate p1 = regex("\\*+");
        assertTrue(p1.matches("*"));
        assertTrue(p1.matches("**"));
        assertFalse(p1.matches(""));

        assertTrue(regex("\\++").matches("+"));
        assertTrue(regex("\\?+").matches("?"));
    }

    @Test
    void pruefungLaeuftAufDemRohenMuster() {
        assertNull(RegexSubset.findForbidden("ab"));
        assertEquals("^", RegexSubset.findForbidden("^ab$"));
    }

    @Test
    void escapterBackslashGefolgtVonBIstErlaubt() {
        Predicate p = regex("\\\\b");
        assertTrue(p.matches("\\b"));
    }

    // ── Was nur Java braucht ──

    @Test
    void klassenKurzformenSindVerbotenWeilSieNichtDasselbeBedeuten() {
        // Rusts \d deckt im Unicode-Modus jede Unicode-Dezimalziffer
        // ab, Javas \d ohne UNICODE_CHARACTER_CLASS nur [0-9]. Statt
        // die Sprachen naeherungsweise anzugleichen, sind die
        // Kurzformen verboten; [0-9] bedeutet in beiden dasselbe.
        for (String pat : new String[] {"\\d+", "\\D", "\\w", "\\W", "\\s", "\\S"}) {
            LoadException e = assertThrows(LoadException.class, () -> regex(pat),
                    pat + " muss abgelehnt werden");
            assertEquals(LoadException.Kind.PREDICATE, e.kind);
        }
        // Die ausgeschriebene Form bleibt erlaubt.
        assertTrue(regex("[0-9]+").matches("123"));
    }

    @Test
    void alternationAufObersterEbeneIstUmschlossen() {
        // matches() wirkt wie Rusts Rahmung \A(?:...)\z: die Alternation
        // bindet nicht ueber die ganze Eingabe hinweg.
        Predicate p = regex("a|b");
        assertTrue(p.matches("a"));
        assertTrue(p.matches("b"));
        assertFalse(p.matches("ab"));
    }

    @Test
    void kaputtesMusterWirdAlsRegexFehlerGemeldet() {
        LoadException e = assertThrows(LoadException.class, () -> regex("a("));
        assertEquals(LoadException.Kind.PREDICATE, e.kind);
        assertTrue(e.getMessage().contains("bad regex"));
    }

    // ── Lesen aus der Formatdarstellung ──

    @Test
    void alleFuenfArtenLesenSich() throws Exception {
        assertNotNull(read("{\"kind\":\"exists\"}"));
        assertTrue(read("{\"kind\":\"equals\",\"value\":\"x\"}").matches("x"));
        assertTrue(read("{\"kind\":\"prefix\",\"value\":\"ab\"}").matches("abc"));
        assertTrue(read("{\"kind\":\"regex\",\"pattern\":\"[0-9]+\"}").matches("42"));
        assertTrue(read("{\"kind\":\"numeric_range\",\"min\":1,\"max\":2}").matches("1.5"));
    }

    @Test
    void unbekannteArtWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> read("{\"kind\":\"starts_with\",\"value\":\"x\"}"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void ueberzaehligesFeldWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> read("{\"kind\":\"exists\",\"value\":\"x\"}"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void fehlendesPflichtfeldWirdAbgelehnt() {
        LoadException e = assertThrows(LoadException.class,
                () -> read("{\"kind\":\"equals\"}"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void grenzeAlsZeichenketteWirdAbgelehnt() {
        // Rust liest min/max ueber serde als f64, ein String faellt
        // dort durch. Java muss genauso streng sein.
        LoadException e = assertThrows(LoadException.class,
                () -> read("{\"kind\":\"numeric_range\",\"min\":\"1\",\"max\":2}"));
        assertEquals(LoadException.Kind.MALFORMED, e.kind);
    }

    @Test
    void fehlenderWertTrifftNurBeiKeinemPraedikatZu() {
        assertFalse(new Predicate.Exists().matches(null));
        assertFalse(new Predicate.Equals("x").matches(null));
        assertFalse(new Predicate.Prefix("x").matches(null));
        assertFalse(regex("a*").matches(null));
        assertFalse(new Predicate.NumericRange(0, 1).matches(null));
    }
}
