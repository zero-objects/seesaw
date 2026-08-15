package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;

import java.util.List;

/**
 * Export gelowerter Erzeugungspläne, Spiegel von
 * {@code seesaw-core/src/rules/export.rs}.
 *
 * <p>Zweck ist der Vergleich: was die beiden Sprachen aus derselben
 * Regeldatei senken, muss dieselbe Zeichenkette ergeben. Deshalb ist
 * das Schema hier nicht frei gewählt, sondern von der Rust-Seite
 * übernommen, bis in die Reihenfolge der Felder.
 */
public final class Export {
    private static final ObjectMapper M = new ObjectMapper();

    private Export() {}

    /** Pläne als eingerückte Zeichenkette, Schema wie in Rust. */
    public static String plansToJson(List<Rule> rules, Graph g) {
        ObjectNode root = M.createObjectNode();
        ArrayNode arr = root.putArray("rules");
        for (Rule r : rules) {
            arr.add(ruleToJson(r, g));
        }
        try {
            return M.writerWithDefaultPrettyPrinter().writeValueAsString(root);
        } catch (Exception e) {
            throw new IllegalStateException("Plan laesst sich nicht serialisieren", e);
        }
    }

    private static ObjectNode ruleToJson(Rule r, Graph g) {
        ObjectNode o = M.createObjectNode();
        o.put("name", r.name);
        o.put("rank", r.rank);
        o.put("direction", r.direction.wire);
        ArrayNode pn = o.putArray("pattern_nodes");
        for (PatNode n : r.patNodes) {
            pn.add(patternNodeToJson(n, g));
        }
        ArrayNode pl = o.putArray("pattern_links");
        for (PatLink l : r.patLinks) {
            pl.add(linkToJson(l));
        }
        ArrayNode cn = o.putArray("create_nodes");
        for (CreateNode c : r.createNodes) {
            cn.add(createNodeToJson(c));
        }
        ArrayNode cl = o.putArray("create_links");
        for (int[] l : r.createLinks) {
            ArrayNode pair = cl.addArray();
            pair.add(refToJson(l[0]));
            pair.add(refToJson(l[1]));
        }
        ArrayNode it = o.putArray("input_types");
        for (String t : r.inputTypes) {
            it.add(t);
        }
        ArrayNode cr = o.putArray("corr_recognition");
        for (Object[] e : r.corrRecognition) {
            ObjectNode c = cr.addObject();
            c.put("corr_type", (String) e[0]);
            c.put("anchor", (Integer) e[1]);
            c.put("endpoint_type", (String) e[2]);
        }
        return o;
    }

    private static ObjectNode patternNodeToJson(PatNode n, Graph g) {
        ObjectNode o = M.createObjectNode();
        o.put("typ", g.typeName(n.typ));
        if (n.predicate == null) {
            o.putNull("value");
        } else {
            o.set("value", predicateToJson(n.predicate));
        }
        return o;
    }

    private static ObjectNode predicateToJson(Predicate p) {
        ObjectNode o = M.createObjectNode();
        if (p instanceof Predicate.Exists) {
            o.put("kind", "exists");
        } else if (p instanceof Predicate.Equals) {
            o.put("kind", "equals");
            o.put("value", ((Predicate.Equals) p).expected());
        } else if (p instanceof Predicate.Prefix) {
            o.put("kind", "prefix");
            o.put("value", ((Predicate.Prefix) p).prefix());
        } else if (p instanceof Predicate.Regex) {
            o.put("kind", "regex");
            // Rusts `Regex::as_str` liefert das GERAHMTE Muster.
            o.put("pattern", ((Predicate.Regex) p).anchored());
        } else if (p instanceof Predicate.NumericRange) {
            Predicate.NumericRange nr = (Predicate.NumericRange) p;
            o.put("kind", "numeric_range");
            o.put("min", nr.min());
            o.put("max", nr.max());
        } else {
            throw new IllegalStateException("unbekanntes Praedikat: " + p);
        }
        return o;
    }

    private static ObjectNode linkToJson(PatLink l) {
        ObjectNode o = M.createObjectNode();
        o.put("from", l.from);
        o.put("to", l.to);
        switch (l.kind) {
            case CONTEXT:
                o.put("kind", "context");
                break;
            case SAME_VALUE:
                o.put("kind", "same_value");
                break;
            default:
                o.put("kind", "directed");
                break;
        }
        return o;
    }

    /** Ref-Kodierung des Plans zurueck in die Rust-Schreibweise. */
    private static ObjectNode refToJson(int ref) {
        ObjectNode o = M.createObjectNode();
        if (ref >= 0) {
            o.put("matched", ref);
        } else {
            o.put("new", -ref - 1);
        }
        return o;
    }

    private static ObjectNode createNodeToJson(CreateNode cn) {
        ObjectNode o = M.createObjectNode();
        o.put("typ", cn.typ);
        o.set("parent", refToJson(cn.parent));
        if (cn.derivedLeaf >= 0) {
            ObjectNode d = M.createObjectNode();
            d.put("source", cn.derivedLeaf);
            d.set("transform", transformToJson(cn.derivedTransform));
            o.set("derived", d);
        } else {
            o.putNull("derived");
        }
        if (cn.konst == null) {
            o.putNull("konst");
        } else {
            o.put("konst", cn.konst);
        }
        if (cn.dynAttr != null) {
            ObjectNode d = M.createObjectNode();
            d.put("anchor", cn.dynAnchor);
            d.put("attr", cn.dynAttr);
            d.set("transform", transformToJson(cn.dynTransform));
            o.set("derived_dyn", d);
        } else {
            o.putNull("derived_dyn");
        }
        o.put("corr_full_match", cn.corrFullMatch);
        return o;
    }

    private static ArrayNode transformToJson(Chain c) {
        ArrayNode arr = M.createArrayNode();
        for (Prim p : c.prims) {
            ObjectNode o = arr.addObject();
            switch (p.op) {
                case IDENTITY:
                    o.put("op", "identity");
                    break;
                case CAPITALIZE:
                    o.put("op", "capitalize");
                    break;
                case DECAPITALIZE:
                    o.put("op", "decapitalize");
                    break;
                case PREFIX:
                    o.put("op", "prefix");
                    o.put("arg", p.arg);
                    break;
                case SUFFIX:
                    o.put("op", "suffix");
                    o.put("arg", p.arg);
                    break;
                case STRIP_PREFIX:
                    o.put("op", "strip_prefix");
                    o.put("arg", p.arg);
                    break;
                default:
                    o.put("op", "strip_suffix");
                    o.put("arg", p.arg);
                    break;
            }
        }
        return arr;
    }
}
