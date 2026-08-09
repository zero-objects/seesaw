package net.sandrakessler.seesaw.rules;

import net.sandrakessler.seesaw.engine.Matcher;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.regex.Pattern;
import java.util.regex.PatternSyntaxException;

/**
 * Wert-Prädikate, Spiegel von {@code rules/predicate.rs}.
 *
 * <p>Fünf Arten: {@code exists}, {@code equals}, {@code prefix},
 * {@code regex}, {@code numeric_range}. Weil zwei Sprachen sie
 * auswerten, normiert das Format vier Punkte (Spec §6): Vollmatch,
 * keine Anker im Muster, eine enge Syntax-Teilmenge, eine eigene
 * Zahlengrammatik.
 *
 * <p>Der Vollmatch entsteht in Java über {@link java.util.regex.Matcher#matches()},
 * das die gesamte Eingabe verlangt, in Rust über die Rahmung
 * {@code \A(?:muster)\z}. Beide behandeln eine Alternation auf oberster
 * Ebene gleich, weil {@code matches()} wie eine umschließende Gruppe
 * wirkt: {@code a|b} trifft weder hier noch dort auf {@code "ab"} zu.
 */
public sealed interface Predicate {

    /** Trifft zu? {@code null} bedeutet: der Knoten trägt keinen Wert. */
    public boolean matches(String value);

    /** Ein Wert ist vorhanden, gleich welcher. */
    public record Exists() implements Predicate {
        @Override public boolean matches(String value) {
            return value != null;
        }
    }

    public record Equals(String expected) implements Predicate {
        @Override public boolean matches(String value) {
            return expected.equals(value);
        }
    }

    public record Prefix(String prefix) implements Predicate {
        @Override public boolean matches(String value) {
            return value != null && value.startsWith(prefix);
        }
    }

    /**
     * {@code raw} ist das Muster, wie es in der Datei steht;
     * {@code pattern} ist daraus uebersetzt und wird ueber
     * {@code matches()} ausgewertet, also als Vollmatch. Rust rahmt
     * stattdessen zu {@code \A(?:...)\z}. {@link #anchored()} bildet
     * diese Rahmung nach, damit beide Seiten beim Export dieselbe
     * Zeichenkette schreiben.
     */
    public record Regex(Pattern pattern, String raw) implements Predicate {
        @Override public boolean matches(String value) {
            return value != null && pattern.matcher(value).matches();
        }

        /** Das Muster in Rusts gerahmter Schreibweise. */
        public String anchored() {
            return "\\A(?:" + raw + ")\\z";
        }
    }

    /** Grenzen inklusive, wie in Rust. */
    public record NumericRange(double min, double max) implements Predicate {
        @Override public boolean matches(String value) {
            Double n = Numbers.parse(value);
            return n != null && n >= min && n <= max;
        }
    }

    /**
     * Muster prüfen und übersetzen. Die Prüfung läuft auf dem ROHEN
     * Muster, nicht auf einer gerahmten Fassung — sonst könnte die
     * Rahmung selbst in die Prüfung geraten. In Java gibt es keine
     * Rahmung, der Vollmatch kommt aus {@code matches()}, aber die
     * Reihenfolge bleibt dieselbe wie in Rust.
     */
    public static Predicate parseRegex(String pattern, String rule, String node) {
        String bad = RegexSubset.findForbidden(pattern);
        if (bad != null) {
            throw LoadException.predicate(rule, node, "forbidden syntax: " + bad);
        }
        try {
            return new Regex(Pattern.compile(pattern), pattern);
        } catch (PatternSyntaxException e) {
            throw LoadException.predicate(rule, node, "bad regex: " + e.getMessage());
        }
    }

    /**
     * Ein Prädikat aus seiner Formatdarstellung. Die erlaubten Felder
     * hängen an der Art, wie bei den Transformations-Primitiven.
     */
    public static Predicate read(JsonNode p, String rule, String node) {
        Json.At at = new Json.At(rule, null, node);
        Json.mustBeObject(p, at, "predicate");
        String kind = Json.requireText(p, "kind", at);
        switch (kind) {
            case "exists":
                Json.allowOnly(p, at, "kind");
                return new Exists();
            case "equals":
                Json.allowOnly(p, at, "kind", "value");
                return new Equals(Json.requireText(p, "value", at));
            case "prefix":
                Json.allowOnly(p, at, "kind", "value");
                return new Prefix(Json.requireText(p, "value", at));
            case "regex":
                Json.allowOnly(p, at, "kind", "pattern");
                return parseRegex(Json.requireText(p, "pattern", at), rule, node);
            case "numeric_range": {
                Json.allowOnly(p, at, "kind", "min", "max");
                double min = readBound(p, "min", at);
                double max = readBound(p, "max", at);
                return new NumericRange(min, max);
            }
            default:
                throw LoadException.malformed(rule, null, node,
                        "unknown predicate kind '" + kind + "', expected one of "
                                + "[exists, equals, prefix, regex, numeric_range]");
        }
    }

    /**
     * Grenze eines Zahlenbereichs: eine JSON-Zahl, nicht ein String.
     *
     * <p>Rust liest {@code min}/{@code max} über serde als {@code f64}
     * (siehe {@code PredNumericRangeArgs}), also direkt aus der
     * JSON-Zahl. Die eigene Zahlengrammatik aus {@link Numbers} gilt
     * für den GEPRÜFTEN WERT aus dem Modell, der als Zeichenkette
     * ankommt, nicht für die Grenzen in der Regeldatei.
     */
    private static double readBound(JsonNode p, String field, Json.At at) {
        JsonNode n = Json.requireField(p, field, at);
        if (!n.isNumber()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be a number");
        }
        return n.asDouble();
    }
}
