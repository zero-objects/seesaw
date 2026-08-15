package net.sandrakessler.seesaw.session;
import net.sandrakessler.seesaw.rules.Predicate;

import net.sandrakessler.seesaw.rules.Chain;
import net.sandrakessler.seesaw.rules.PrimOp;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Prim;
import net.sandrakessler.seesaw.rules.Transform;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.IOException;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.regex.Pattern;

/** Lädt die aus Rust exportierten Fixtures (gelowerte Regeln, Seeds). */
public final class Fixtures {
    private static final ObjectMapper M = new ObjectMapper();

    private Fixtures() {}

    public static JsonNode resource(String path) throws IOException {
        try (InputStream in = Fixtures.class.getResourceAsStream(path)) {
            if (in == null) throw new IOException("Fixture fehlt: " + path);
            return M.readTree(in);
        }
    }

    /** Transformation eines Fixtures: eine Kette aus Primitiven. */
    public static Chain transform(JsonNode n) {
        if (!n.isArray()) {
            throw new IllegalArgumentException(
                    "transform muss eine Liste von Primitiven sein: " + n);
        }
        List<Prim> prims = new ArrayList<>();
        for (JsonNode p : n) prims.add(prim(p));
        return Chain.chain(prims);
    }

    public static Prim prim(JsonNode p) {
        String arg = p.has("arg") ? p.get("arg").asText() : "";
        switch (p.get("op").asText()) {
            case "identity": return new Prim(PrimOp.IDENTITY);
            case "capitalize": return new Prim(PrimOp.CAPITALIZE);
            case "decapitalize": return new Prim(PrimOp.DECAPITALIZE);
            case "prefix": return new Prim(PrimOp.PREFIX, arg);
            case "suffix": return new Prim(PrimOp.SUFFIX, arg);
            case "strip_prefix": return new Prim(PrimOp.STRIP_PREFIX, arg);
            case "strip_suffix": return new Prim(PrimOp.STRIP_SUFFIX, arg);
            default: throw new IllegalArgumentException(p.get("op").asText());
        }
    }

    public static PatNode patNode(JsonNode n, Graph g) {
        int typ = g.intern(n.get("typ").asText());
        JsonNode v = n.get("value");
        if (v == null || v.isNull()) {
            return new PatNode(typ, null);
        }
        return new PatNode(typ, Predicate.read(v, "fixture", n.get("typ").asText()));
    }

    public static int ref(JsonNode n) {
        // {"m": pos} → pos; {"n": ix} → -ix-1
        if (n.has("m")) return n.get("m").asInt();
        return -n.get("n").asInt() - 1;
    }

    /** Regeln einer Phase; Typ-Interning im übergebenen Graphen. */
    public static List<Rule> rules(String phase, Graph g) throws IOException {
        JsonNode root = resource("/fixtures/rules_" + phase + ".json");
        List<Rule> out = new ArrayList<>();
        for (JsonNode rn : root.get("rules")) {
            Rule r = new Rule();
            r.name = rn.get("name").asText();
            r.rank = rn.get("rank").asLong();
            JsonNode direction = rn.get("direction");
            if (direction != null) {
                r.direction = Rule.Direction.fromWire(direction.asText());
            } else if (r.name.endsWith("→")) {
                r.direction = Rule.Direction.FORWARD;
            } else if (r.name.endsWith("←")) {
                r.direction = Rule.Direction.BACKWARD;
            }
            r.patNodes = new ArrayList<>();
            for (JsonNode n : rn.get("pattern_nodes")) r.patNodes.add(patNode(n, g));
            r.patLinks = new ArrayList<>();
            for (JsonNode l : rn.get("pattern_links"))
                r.patLinks.add(new PatLink(l.get("from").asInt(), l.get("to").asInt(),
                        l.get("context").asBoolean()));
            r.createNodes = new ArrayList<>();
            for (JsonNode cn : rn.get("create_nodes")) {
                JsonNode d = cn.get("derived");
                int leaf = -1;
                Chain t = null;
                if (d != null && !d.isNull()) {
                    leaf = d.get("leaf").asInt();
                    t = transform(d.get("transform"));
                }
                JsonNode kn = cn.get("konst");
                String konst = (kn == null || kn.isNull()) ? null : kn.asText();
                JsonNode dd = cn.get("derived_dyn");
                int dynAnchor = -1;
                String dynAttr = null;
                Chain dynT = null;
                if (dd != null && !dd.isNull()) {
                    dynAnchor = dd.get("anchor").asInt();
                    dynAttr = dd.get("attr").asText();
                    dynT = transform(dd.get("transform"));
                }
                boolean cfm = cn.has("corr_full_match") && cn.get("corr_full_match").asBoolean();
                r.createNodes.add(new CreateNode(cn.get("typ").asText(),
                        ref(cn.get("parent")), leaf, t, konst, dynAnchor, dynAttr, dynT, cfm));
            }
            r.createLinks = new ArrayList<>();
            for (JsonNode cl : rn.get("create_links"))
                r.createLinks.add(new int[] { ref(cl.get(0)), ref(cl.get(1)) });
            r.inputTypes = new ArrayList<>();
            for (JsonNode t : rn.get("input_types")) r.inputTypes.add(t.asText());
            JsonNode cr = rn.get("corr_recognition");
            if (cr != null && !cr.isNull()) {
                for (JsonNode e : cr) {
                    r.corrRecognition.add(new Object[] {
                        e.get(0).asText(), e.get(1).asInt(), e.get(2).asText(),
                    });
                }
            }
            out.add(r);
        }
        return out;
    }

    /** Seed-Graph + Werte (Ids aus dem Export, Typen frisch interniert). */
    public static Map<Id, String> seed(String name, Graph g) throws IOException {
        return seedFromNode(resource("/fixtures/seed_" + name + ".json"), g);
    }

    /** Seed aus beliebigem JSON-Baum (Datei-Pfad für Skalen-Sweep). */
    public static Map<Id, String> seedFromNode(JsonNode root, Graph g) {
        for (JsonNode n : root.get("nodes")) {
            Id id = Id.fromHex(n.get("id").asText());
            g.insertNode(new Node(id, g.intern(n.get("typ").asText()), St.SOLID, null, null));
        }
        for (JsonNode c : root.get("connections")) {
            g.connect(Id.fromHex(c.get(0).asText()), Id.fromHex(c.get(1).asText()),
                    St.SOLID);
        }
        Map<Id, String> values = new HashMap<>();
        for (JsonNode v : root.get("values"))
            values.put(Id.fromHex(v.get(0).asText()), v.get(1).asText());
        return values;
    }
}
