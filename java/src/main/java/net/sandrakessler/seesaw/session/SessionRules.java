package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.plan.Rule;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;

import java.io.IOException;
import java.io.InputStream;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

import net.sandrakessler.seesaw.session.Fixtures;

/**
 * Regelsatz + Adapter-Metadaten einer Java-nativen Engine-Session (E2).
 *
 * <p>Die gelowerten Regeln kommen über {@link Fixtures#rules} aus dem
 * Fixture-Export ({@code /fixtures/rules_<name>.json}); zusätzlich trägt
 * das Fixture drei mechanisch aus dem alten Regelsatz abgeleitete
 * Tabellen, die die Δ-JSON⇄Codecs brauchen:
 * <ul>
 *   <li>{@code attr_types} — Attributnamen des Regelsatzes; im
 *       Modell sind das Blatt-Subknoten-Typen (Spec: Knoten-Attr →
 *       Blatt), im Δ-/Snapshot-JSON dagegen {@code attrs}-Einträge.</li>
 *   <li>{@code corr_types} — Correspondence-Typen; ihre Verbindungen
 *       erscheinen im Snapshot als {@code corrL}/{@code corrR}-Kanten
 *       (alten Wire-Namen, siehe seesaw-core {@code CORR_EDGE_KINDS}).</li>
 *   <li>{@code edge_combos} — (Quelltyp, Kantentyp, Zieltyp) je
 *       Kante der ersten Generation samt Direct/Reified-Entscheidung (Prüfpunkt-2-Tabelle
 *       aus dem Konverter der ersten Generation); Direct-Kanten sind
 *       hier anonym, der Kantentyp wird beim Snapshot aus dem
 *       Endpunkt-Typ-Paar rekonstruiert.</li>
 * </ul>
 */
public final class SessionRules {

    private static final ObjectMapper M = new ObjectMapper();

    public final List<Rule> rules;
    /** Attributnamen (= Blatt-Typen im Graphenen). */
    public final Set<String> attrTypes;
    /** Correspondence-Typen (Snapshot: corrL/corrR-Kanten). */
    public final Set<String> corrTypes;
    /** (Quelltyp, Zieltyp) → alten Kantentyp, nur Direct-Kanten. */
    public final Map<String, String> edgeKindByCombo;
    /** Kantentypen mit Reified-Entscheidung (Rollen-Knoten). */
    public final Set<String> reifiedKinds;

    public SessionRules(List<Rule> rules, Set<String> attrTypes,
            Set<String> corrTypes, Map<String, String> edgeKindByCombo,
            Set<String> reifiedKinds) {
        this.rules = rules;
        this.attrTypes = attrTypes;
        this.corrTypes = corrTypes;
        this.edgeKindByCombo = edgeKindByCombo;
        this.reifiedKinds = reifiedKinds;
    }

    /** Schlüssel des Endpunkt-Typ-Paars (NUL-getrennt, kollisionsfrei). */
    public static String comboKey(String sourceTyp, String targetTyp) {
        return sourceTyp + '\0' + targetTyp;
    }

    /**
     * Lädt Regeln + Tabellen aus {@code /fixtures/rules_<name>.json}.
     * Typ-Interning läuft im übergebenen Graphen (wie Fixtures).
     */
    public static SessionRules load(String name, Graph g) throws IOException {
        List<Rule> rules = Fixtures.rules(name, g);

        String path = "/rules/rules_" + name + ".json";
        JsonNode root;
        try (InputStream in = SessionRules.class.getResourceAsStream(path)) {
            if (in == null) throw new IOException("Fixture fehlt: " + path);
            root = M.readTree(in);
        }

        Set<String> attrTypes = new HashSet<>();
        for (JsonNode t : root.path("attr_types")) attrTypes.add(t.asText());

        Set<String> corrTypes = new HashSet<>();
        for (JsonNode t : root.path("corr_types")) corrTypes.add(t.asText());
        // Fallback ohne explizite Tabelle: corr_recognition nennt jeden
        // Establishes-Corr-Typ.
        for (Rule r : rules)
            for (Object[] rec : r.corrRecognition) corrTypes.add((String) rec[0]);

        Map<String, String> combos = new HashMap<>();
        Set<String> reified = new HashSet<>();
        for (JsonNode e : root.path("edge_combos")) {
            String src = e.get(0).asText();
            String kind = e.get(1).asText();
            String tgt = e.get(2).asText();
            boolean isReified = e.size() > 3 && "reified".equals(e.get(3).asText());
            if (isReified) {
                reified.add(kind);
            } else {
                combos.put(comboKey(src, tgt), kind);
            }
        }
        return new SessionRules(rules, attrTypes, corrTypes, combos, reified);
    }
}
