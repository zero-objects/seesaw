package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.util.ArrayList;
import java.util.List;

/**
 * Die Dateistruktur des Regelformats, Spiegel von
 * {@code seesaw-core/src/rules/format.rs}.
 *
 * <p>Reines Lesen, keine Logik: was hier durchkommt, ist syntaktisch
 * eine Regeldatei. Ob sie in sich stimmt, entscheidet {@link Validate}.
 *
 * <p>Rust bekommt die Strenge von serde: {@code deny_unknown_fields} an
 * jeder Struktur, Pflichtfelder über den Typ. Java liest über
 * Jackson-Bäume und prüft beides von Hand über {@link Json}, sonst wäre
 * dieser Leser nachsichtiger als der andere.
 */
public final class Format {
    private static final ObjectMapper MAPPER = new ObjectMapper();

    private Format() {}

    /** Formatversion im Kopf jeder Datei. */
    public static final int FORMAT_VERSION = 3;

    // ── Datenklassen ──

    /** Eine Datei: Kopf plus Regeln. */
    public static final class RuleFile {
        public final int format;
        public final String name;
        public final List<RuleDecl> rules;

        RuleFile(int format, String name, List<RuleDecl> rules) {
            this.format = format;
            this.name = name;
            this.rules = rules;
        }
    }

    /** Eine bidirektionale Regel. */
    public static final class RuleDecl {
        public final String name;
        public final long rank;
        /** Frei, nur für Menschen; geht nicht in die Identität ein. */
        public final String documentation;
        public final SideDecl left;
        public final SideDecl right;
        public final List<CorrDecl> corrs;
        /** Wert-Gleichheit ÜBER die Seiten hinweg: (linker, rechter Name). */
        public final List<String[]> joins;

        RuleDecl(String name, long rank, String documentation, SideDecl left,
                SideDecl right, List<CorrDecl> corrs, List<String[]> joins) {
            this.name = name;
            this.rank = rank;
            this.documentation = documentation;
            this.left = left;
            this.right = right;
            this.corrs = corrs;
            this.joins = joins;
        }
    }

    /** Eine Seite: benannte Knoten, Verbindungen, Wert-Gleichheiten. */
    public static final class SideDecl {
        public final String anchor;
        public final List<NodeDecl> nodes;
        public final List<String[]> links;
        /** Wert-Gleichheit INNERHALB der Seite. */
        public final List<String[]> sameValueLinks;

        SideDecl(String anchor, List<NodeDecl> nodes, List<String[]> links,
                List<String[]> sameValueLinks) {
            this.anchor = anchor;
            this.nodes = nodes;
            this.links = links;
            this.sameValueLinks = sameValueLinks;
        }
    }

    /** Ein Knoten. */
    public static final class NodeDecl {
        public final String name;
        public final String typ;
        /** Wert-Bedingung beim Matchen, null = keine. */
        public final Predicate predicate;
        /** Kontext: wird gematcht, nie erzeugt. */
        public final boolean context;
        /** Dieser rechte Knoten IST ein linker (Same-Domain-Kontext). */
        public final String sameAs;
        /** Regel-Konstante: der Wert steht in der Regel, null = keine. */
        public final String constant;

        NodeDecl(String name, String typ, Predicate predicate, boolean context,
                String sameAs, String constant) {
            this.name = name;
            this.typ = typ;
            this.predicate = predicate;
            this.context = context;
            this.sameAs = sameAs;
            this.constant = constant;
        }
    }

    /** Rolle einer Korrespondenz. */
    public enum Role {
        /** Diese Corr wird von der Regel erzeugt. */
        ESTABLISHES,
        /** Diese Corr muss schon da sein; sie liefert Kontext. */
        REFERENCES;

        static Role read(String s, Json.At at) {
            switch (s) {
                case "establishes":
                    return ESTABLISHES;
                case "references":
                    return REFERENCES;
                default:
                    throw LoadException.malformed(at.rule, at.side, at.name,
                            "unknown role '" + s + "', expected 'establishes' or 'references'");
            }
        }
    }

    /** Eine Korrespondenz zwischen einem linken und einem rechten Knoten. */
    public static final class CorrDecl {
        public final String typ;
        public final String left;
        public final String right;
        public final Role role;
        public final List<BindingDecl> bindings;

        CorrDecl(String typ, String left, String right, Role role, List<BindingDecl> bindings) {
            this.typ = typ;
            this.left = left;
            this.right = right;
            this.role = role;
            this.bindings = bindings;
        }
    }

    /**
     * Eine Wert-Bindung. Jede Seite ist ENTWEDER ein Knotenname
     * (statisch) ODER ein Blatt-Typname (dynamisch, am Anker gesucht),
     * nie beides und nie keines. Was davon gilt, prüft {@link Validate}.
     */
    public static final class BindingDecl {
        public final String left;
        public final String right;
        public final String leftType;
        public final String rightType;
        public final Chain transform;

        BindingDecl(String left, String right, String leftType, String rightType,
                Chain transform) {
            this.left = left;
            this.right = right;
            this.leftType = leftType;
            this.rightType = rightType;
            this.transform = transform;
        }
    }

    // ── Lesen ──

    /** Aus Text. Wirft {@link LoadException} bei allem, was nicht passt. */
    public static RuleFile fromJson(String json) {
        JsonNode root;
        try {
            root = MAPPER.readTree(json);
        } catch (Exception e) {
            throw LoadException.malformed(null, null, null, "not valid JSON: " + e.getMessage());
        }
        return read(root);
    }

    /** Aus einem schon geparsten Baum. */
    public static RuleFile read(JsonNode root) {
        Json.At at = Json.At.none();
        Json.mustBeObject(root, at, "rule file");
        Json.allowOnly(root, at, "format", "name", "rules");
        int format = Json.requireInt(root, "format", at);
        if (format != FORMAT_VERSION) {
            throw LoadException.version(format, FORMAT_VERSION);
        }
        String name = Json.optionalText(root, "name", at);
        List<RuleDecl> rules = new ArrayList<>();
        for (JsonNode r : Json.requireArray(root, "rules", at)) {
            rules.add(readRule(r));
        }
        return new RuleFile(format, name == null ? "" : name, rules);
    }

    private static RuleDecl readRule(JsonNode r) {
        Json.At at = Json.At.none();
        Json.mustBeObject(r, at, "rule");
        String name = Json.requireText(r, "name", at);
        at = at.rule(name);
        Json.allowOnly(r, at, "name", "rank", "documentation", "left", "right", "corrs", "joins");
        long rank = Json.requireInt(r, "rank", at);
        String doc = Json.optionalText(r, "documentation", at);
        SideDecl left = readSide(Json.requireObject(r, "left", at), at.side("left"));
        SideDecl right = readSide(Json.requireObject(r, "right", at), at.side("right"));
        List<CorrDecl> corrs = new ArrayList<>();
        JsonNode cs = r.get("corrs");
        if (cs != null && !cs.isNull()) {
            if (!cs.isArray()) {
                throw LoadException.malformed(name, null, null, "field 'corrs' must be an array");
            }
            for (JsonNode c : cs) {
                corrs.add(readCorr(c, at));
            }
        }
        return new RuleDecl(name, rank, doc, left, right, corrs, readPairs(r, "joins", at));
    }

    private static SideDecl readSide(JsonNode s, Json.At at) {
        Json.allowOnly(s, at, "anchor", "nodes", "links", "same_value_links");
        String anchor = Json.requireText(s, "anchor", at);
        List<NodeDecl> nodes = new ArrayList<>();
        for (JsonNode n : Json.requireArray(s, "nodes", at)) {
            nodes.add(readNode(n, at));
        }
        return new SideDecl(anchor, nodes, readPairs(s, "links", at),
                readPairs(s, "same_value_links", at));
    }

    private static NodeDecl readNode(JsonNode n, Json.At at) {
        Json.mustBeObject(n, at, "node");
        String name = Json.requireText(n, "name", at);
        Json.At nat = at.name(name);
        Json.allowOnly(n, nat, "name", "type", "predicate", "context", "same_as", "constant");
        String typ = Json.requireText(n, "type", nat);
        JsonNode p = n.get("predicate");
        Predicate pred = (p == null || p.isNull()) ? null : Predicate.read(p, at.rule, name);
        JsonNode ctx = n.get("context");
        if (ctx != null && !ctx.isNull() && !ctx.isBoolean()) {
            throw LoadException.malformed(at.rule, at.side, name, "field 'context' must be a boolean");
        }
        return new NodeDecl(name, typ, pred, ctx != null && ctx.asBoolean(),
                Json.optionalText(n, "same_as", nat), Json.optionalText(n, "constant", nat));
    }

    private static CorrDecl readCorr(JsonNode c, Json.At at) {
        Json.mustBeObject(c, at, "corr");
        Json.allowOnly(c, at, "type", "left", "right", "role", "bindings");
        String typ = Json.requireText(c, "type", at);
        Json.At cat = at.name(typ);
        Role role = Role.read(Json.requireText(c, "role", cat), cat);
        List<BindingDecl> bindings = new ArrayList<>();
        JsonNode bs = c.get("bindings");
        if (bs != null && !bs.isNull()) {
            if (!bs.isArray()) {
                throw LoadException.malformed(at.rule, null, typ,
                        "field 'bindings' must be an array");
            }
            for (JsonNode b : bs) {
                bindings.add(readBinding(b, cat));
            }
        }
        return new CorrDecl(typ, Json.requireText(c, "left", cat),
                Json.requireText(c, "right", cat), role, bindings);
    }

    private static BindingDecl readBinding(JsonNode b, Json.At at) {
        Json.mustBeObject(b, at, "binding");
        Json.allowOnly(b, at, "left", "right", "left_type", "right_type", "transform");
        return new BindingDecl(
                Json.optionalText(b, "left", at),
                Json.optionalText(b, "right", at),
                Json.optionalText(b, "left_type", at),
                Json.optionalText(b, "right_type", at),
                Transform.readChain(b.get("transform"), at));
    }

    /** Liste von Paaren, etwa {@code [["a","b"],["c","d"]]}. */
    private static List<String[]> readPairs(JsonNode owner, String field, Json.At at) {
        List<String[]> out = new ArrayList<>();
        JsonNode arr = owner.get(field);
        if (arr == null || arr.isNull()) {
            return out;
        }
        if (!arr.isArray()) {
            throw LoadException.malformed(at.rule, at.side, at.name,
                    "field '" + field + "' must be an array of pairs");
        }
        for (JsonNode pair : arr) {
            if (!pair.isArray() || pair.size() != 2 || !pair.get(0).isTextual()
                    || !pair.get(1).isTextual()) {
                throw LoadException.malformed(at.rule, at.side, at.name,
                        "field '" + field + "' must contain pairs of two strings");
            }
            out.add(new String[] {pair.get(0).asText(), pair.get(1).asText()});
        }
        return out;
    }
}
