package net.sandrakessler.seesaw.rules;

/**
 * Ladefehler einer Regeldatei, mit Fundstelle.
 *
 * <p>Spiegel von {@code seesaw-core/src/rules/validate.rs::LoadError}. Die
 * Aufzählung {@link Kind} trägt dieselben Fälle in derselben
 * Unterscheidungstiefe, damit ein Test in beiden Sprachen dieselbe
 * Ablehnung prüfen kann.
 *
 * <p>Die Meldungstexte sind englisch wie auf der Rust-Seite. Sie gehen
 * in ein veröffentlichtes Artefakt und werden von Leuten gelesen, die
 * den deutschen Kommentar daneben nicht sehen.
 */
public final class LoadException extends RuntimeException {
    private static final long serialVersionUID = 1L;

    /** Fehlerfall. Namensgleich mit den Rust-Varianten. */
    public enum Kind {
        VERSION,
        DUPLICATE_RULE_NAME,
        DUPLICATE_NODE,
        DUPLICATE_LINK,
        DUPLICATE_SAME_VALUE_LINK,
        UNKNOWN_NODE,
        UNKNOWN_ANCHOR,
        SAME_AS_ON_LEFT,
        UNKNOWN_SAME_AS,
        AMBIGUOUS_BINDING,
        EMPTY_BINDING,
        MIXED_BINDING,
        PREDICATE,
        PREDICATE_ON_CREATED_NODE,
        CONSTANT_PREDICATE_MISMATCH,
        CONSTANT_ON_MATCHED_NODE,
        /**
         * Die Datei ist schon syntaktisch keine Regeldatei: unbekanntes
         * Feld, fehlendes Pflichtfeld, falscher Typ, unbekannte
         * Vokabel. Auf der Rust-Seite kommt das nicht aus
         * {@code LoadError}, sondern aus serde, weil das Parsen dort
         * ein eigener Schritt vor der Validierung ist. Java liest über
         * Jackson-Bäume und braucht deshalb einen eigenen Fall.
         */
        MALFORMED,
    }

    /** Fehlerfall. */
    public final Kind kind;
    /** Regelname, oder null wenn der Fehler nicht an einer Regel hängt. */
    public final String rule;
    /** {@code "left"} oder {@code "right"}, oder null. */
    public final String side;
    /** Betroffener Knoten-, Corr- oder Regelname, oder null. */
    public final String name;

    private LoadException(Kind kind, String rule, String side, String name, String message) {
        super(message);
        this.kind = kind;
        this.rule = rule;
        this.side = side;
        this.name = name;
    }

    private static String at(String rule, String side, String name) {
        StringBuilder b = new StringBuilder();
        if (rule != null) b.append(" in rule '").append(rule).append('\'');
        if (side != null) b.append(", side ").append(side);
        if (name != null) b.append(", at '").append(name).append('\'');
        return b.toString();
    }

    public static LoadException version(int found, int expected) {
        return new LoadException(Kind.VERSION, null, null, null,
                "unsupported format version " + found + ", expected " + expected);
    }

    public static LoadException duplicateRuleName(String name) {
        return new LoadException(Kind.DUPLICATE_RULE_NAME, name, null, name,
                "duplicate rule name '" + name + "' — the rule name enters identity");
    }

    public static LoadException duplicateNode(String rule, String side, String name) {
        return new LoadException(Kind.DUPLICATE_NODE, rule, side, name,
                "duplicate node name" + at(rule, side, name));
    }

    public static LoadException duplicateLink(String rule, String side, String a, String b) {
        return new LoadException(Kind.DUPLICATE_LINK, rule, side, a,
                "duplicate link (" + a + ", " + b + ")" + at(rule, side, null));
    }

    public static LoadException duplicateSameValueLink(
            String rule, String side, String a, String b) {
        return new LoadException(Kind.DUPLICATE_SAME_VALUE_LINK, rule, side, a,
                "duplicate same_value link (" + a + ", " + b + ")" + at(rule, side, null));
    }

    public static LoadException unknownNode(String rule, String side, String name) {
        return new LoadException(Kind.UNKNOWN_NODE, rule, side, name,
                "unknown node name" + at(rule, side, name));
    }

    public static LoadException unknownAnchor(String rule, String side, String name) {
        return new LoadException(Kind.UNKNOWN_ANCHOR, rule, side, name,
                "unknown or missing anchor" + at(rule, side, name));
    }

    public static LoadException sameAsOnLeft(String rule, String name) {
        return new LoadException(Kind.SAME_AS_ON_LEFT, rule, "left", name,
                "same_as is only allowed on the right side" + at(rule, "left", name));
    }

    public static LoadException unknownSameAs(String rule, String name) {
        return new LoadException(Kind.UNKNOWN_SAME_AS, rule, "right", name,
                "same_as points at an unknown left node" + at(rule, "right", name));
    }

    public static LoadException ambiguousBinding(String rule, String corr) {
        return new LoadException(Kind.AMBIGUOUS_BINDING, rule, null, corr,
                "binding has both a static and a dynamic source" + at(rule, null, corr));
    }

    public static LoadException emptyBinding(String rule, String corr) {
        return new LoadException(Kind.EMPTY_BINDING, rule, null, corr,
                "binding has no source at all" + at(rule, null, corr));
    }

    public static LoadException mixedBinding(String rule, String corr) {
        return new LoadException(Kind.MIXED_BINDING, rule, null, corr,
                "binding mixes a node name on one side with a leaf type on the other"
                        + at(rule, null, corr));
    }

    public static LoadException predicate(String rule, String node, String detail) {
        return new LoadException(Kind.PREDICATE, rule, null, node,
                "invalid predicate: " + detail + at(rule, null, node));
    }

    public static LoadException predicateOnCreatedNode(String rule, String side, String node) {
        return new LoadException(Kind.PREDICATE_ON_CREATED_NODE, rule, side, node,
                "value predicate on a node that lowering creates — it would never be read"
                        + at(rule, side, node));
    }

    public static LoadException constantPredicateMismatch(String rule, String side, String node) {
        return new LoadException(Kind.CONSTANT_PREDICATE_MISMATCH, rule, side, node,
                "equality predicate and constant disagree on the same created node"
                        + at(rule, side, node));
    }

    public static LoadException constantOnMatchedNode(String rule, String side, String node) {
        return new LoadException(Kind.CONSTANT_ON_MATCHED_NODE, rule, side, node,
                "constant on a node that lowering never creates" + at(rule, side, node));
    }

    public static LoadException malformed(String rule, String side, String name, String detail) {
        return new LoadException(Kind.MALFORMED, rule, side, name,
                detail + at(rule, side, name));
    }

    @Override
    public String toString() {
        return "LoadException[" + kind + "] " + getMessage();
    }
}
