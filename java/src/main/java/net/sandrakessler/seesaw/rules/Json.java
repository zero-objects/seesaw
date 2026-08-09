package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;

import java.util.Arrays;
import java.util.Iterator;

/**
 * Strenges Lesen von Jackson-Bäumen.
 *
 * <p>Rust bekommt die Strenge geschenkt: serde lehnt ein unbekanntes
 * Feld über {@code deny_unknown_fields} ab, ein fehlendes Pflichtfeld
 * über den Typ. Wer über Jackson-Bäume liest, muss beides von Hand
 * prüfen, sonst ist der Java-Loader nachsichtiger als der Rust-Loader
 * und nimmt Dateien an, die Rust zurückweist.
 *
 * <p>Jede Methode wirft {@link LoadException} mit
 * {@link LoadException.Kind#MALFORMED} und der Fundstelle.
 */
final class Json {
    private Json() {}

    /** Wo im Baum wir gerade sind, für die Fehlermeldung. */
    static final class At {
        final String rule;
        final String side;
        final String name;

        At(String rule, String side, String name) {
            this.rule = rule;
            this.side = side;
            this.name = name;
        }

        static At none() { return new At(null, null, null); }

        At rule(String r) { return new At(r, side, name); }

        At side(String s) { return new At(rule, s, name); }

        At name(String n) { return new At(rule, side, n); }
    }

    /** Lehnt jedes Feld ab, das nicht in {@code allowed} steht. */
    public static void allowOnly(JsonNode obj, At at, String... allowed) {
        for (Iterator<String> it = obj.fieldNames(); it.hasNext();) {
            String f = it.next();
            if (!Arrays.asList(allowed).contains(f)) {
                throw LoadException.malformed(at.rule, at.side, at.name,
                        "unknown field '" + f + "', expected one of "
                                + Arrays.toString(allowed));
            }
        }
    }

    public static JsonNode requireField(JsonNode obj, String field, At at) {
        JsonNode n = obj.get(field);
        if (n == null || n.isNull()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "missing required field '" + field + "'");
        }
        return n;
    }

    public static String requireText(JsonNode obj, String field, At at) {
        JsonNode n = requireField(obj, field, at);
        if (!n.isTextual()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be a string");
        }
        return n.asText();
    }

    public static int requireInt(JsonNode obj, String field, At at) {
        JsonNode n = requireField(obj, field, at);
        if (!n.isIntegralNumber()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be an integer");
        }
        return n.asInt();
    }

    public static JsonNode requireArray(JsonNode obj, String field, At at) {
        JsonNode n = requireField(obj, field, at);
        if (!n.isArray()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be an array");
        }
        return n;
    }

    public static JsonNode requireObject(JsonNode obj, String field, At at) {
        JsonNode n = requireField(obj, field, at);
        if (!n.isObject()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be an object");
        }
        return n;
    }

    /** Optionales Textfeld; null wenn nicht da. Typ wird trotzdem geprüft. */
    public static String optionalText(JsonNode obj, String field, At at) {
        JsonNode n = obj.get(field);
        if (n == null || n.isNull()) return null;
        if (!n.isTextual()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be a string");
        }
        return n.asText();
    }

    public static void mustBeObject(JsonNode n, At at, String what) {
        if (!n.isObject()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    what + " must be an object");
        }
    }
}
