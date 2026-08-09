package net.sandrakessler.seesaw.rules;

import com.fasterxml.jackson.databind.JsonNode;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.plan.Rule;

import java.util.ArrayList;
import java.util.List;

/**
 * Der eine Weg von einer Regeldatei zu Erzeugungsplänen, namensgleich
 * zu {@code seesaw_tgg::rules::load}.
 *
 * <p>Drei Schritte: parsen, validieren, senken. Die Klassen darunter
 * sind öffentlich, damit ein Aufrufer Fehler je Schritt melden kann.
 *
 * <p>Typen werden dabei in {@code g} interniert, also muss der Graph,
 * gegen den die Regeln später laufen, derselbe sein.
 */
public final class Rules {
    private Rules() {}

    /** Aus Text. Vorwärts und rückwärts je Regel, in Deklarationsreihenfolge. */
    public static List<Rule> load(String json, Graph g) {
        return loadFile(Format.fromJson(json), g);
    }

    /** Aus einem schon geparsten Baum. */
    public static List<Rule> load(JsonNode json, Graph g) {
        return loadFile(Format.read(json), g);
    }

    /** Aus einer schon gelesenen Datei. */
    public static List<Rule> loadFile(Format.RuleFile file, Graph g) {
        return Lower.lowerAll(Validate.validate(file), g);
    }

    /** Nur die Vorwärtsrichtung, für Aufrufer, die nie rückwärts fahren. */
    public static List<Rule> loadForward(String json, Graph g) {
        List<Rule> all = load(json, g);
        List<Rule> out = new ArrayList<>(all.size() / 2);
        for (int i = 0; i < all.size(); i += 2) {
            out.add(all.get(i));
        }
        return out;
    }
}
