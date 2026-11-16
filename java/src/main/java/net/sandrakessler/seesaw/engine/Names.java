package net.sandrakessler.seesaw.engine;

import java.util.Arrays;
import java.util.HexFormat;
import java.util.List;
import java.util.Objects;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Node;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Chain;

/**
 * Die Namen, die die Pläne der Regeln einer Engine nennen, mit den
 * Kennungen, zu denen sie im Graphen aufgelöst wurden (Rust
 * {@code plan/names.rs}). Der Typ jedes Knotens, den eine Regel erzeugt,
 * die Konstante ihrer Konstanten-Blätter, der Quelltyp eines
 * dynamischen Blatts, der Typ seiner Attribut-Korrespondenz und die
 * beiden Typen je Eintrag der {@code corrRecognition} werden in den
 * Tabellen eines Graphen einmal aufgelöst und je Regel und Planposition
 * behalten. 2.0.2 löste sie bei jeder Nutzung auf. Eine Tabelle der
 * Ketten hat der Java-Graph nicht (ein Knoten trägt seine Kette selbst),
 * dafür ist nichts aufzulösen.
 *
 * <p>Ein Name wird zu dem Zeitpunkt aufgelöst, zu dem 2.0.2 ihn zum
 * ersten Mal auflöste. Die Tabellen bekommen ihre Einträge also in
 * derselben Reihenfolge, und jede Kennung ist dieselbe. Ein Name, den
 * der Graph nur nachschlägt ({@link Graph#lookup}), wird nachgeschlagen,
 * bis der Graph ihn kennt. Behalten wird eine Aussage über die Tabellen
 * eines Graphen: sie wachsen nur, und jeder Graph trägt eine eigene
 * Nummer ({@link Graph#uid()}, eine Kopie eine andere). Die Kennungen
 * entfallen, wenn der Schritt einen anderen Graphen bekommt oder die
 * Regelliste eine andere ist oder ihre Länge geändert hat. Eine Regel,
 * die in der Liste an ihrem Platz ersetzt oder verändert wird, sieht die
 * Prüfung nicht (wie beim {@link AnchorIndex} der Engine).
 *
 * <p>Unter {@link Engine#CHECKS} wird jede behaltene Kennung bei ihrer
 * Nutzung gegen die Auflösung von 2.0.2 geprüft, eine Abweichung wirft.
 *
 * <p>{@link #forStep} wählt die Regel eines Schritts. Die übrigen
 * Methoden gelten für diese Regel bis zum nächsten {@link #forStep}.
 */
final class Names {
    /** Nicht aufgelöst. */
    static final int NONE = -1;
    private static final int[] NO_INTS = new int[0];
    private static final String[] NO_NAMES = new String[0];

    /** Der Graph, in dessen Tabellen die Kennungen aufgelöst sind. 0:
     *  keiner (ein Graph hat nie die Nummer 0). */
    private long graph;
    /** Die Regelliste, zu der die Einträge gehören, und ihre Länge. */
    private List<Rule> rules;
    private int ruleCount;
    /** Je Regel, wo ihre Plätze in den Feldern der Erzeugung und der
     *  Erkennung beginnen, und ein Eintrag mehr für das Ende. */
    int[] createAt = NO_INTS;
    int[] recognitionAt = NO_INTS;
    /** Je Knoten jedes Erzeugungsplans, Regel nach Regel: sein Typ, der
     *  Typ des Quellblatts eines dynamischen Blatts, die Konstante eines
     *  Konstanten-Blatts, Name und Typ der Attribut-Korrespondenz eines
     *  dynamischen Blatts. */
    int[] typ = NO_INTS;
    int[] attr = NO_INTS;
    int[] konst = NO_INTS;
    String[] attrCorrName = NO_NAMES;
    int[] attrCorrTyp = NO_INTS;
    /** Je Eintrag jeder {@code corrRecognition}, Regel nach Regel:
     *  Korrespondenztyp und Typ des Gegenendes. */
    int[] corrTyp = NO_INTS;
    int[] endpointTyp = NO_INTS;
    /** Anfang der Plätze der Regel des laufenden Schritts. */
    private int create0, recognition0;

    /** Wählt die Regel {@code ruleIx} von {@code rules} für einen Schritt
     *  gegen {@code g}. */
    Names forStep(Graph g, List<Rule> rules, int ruleIx) {
        if (graph != g.uid() || this.rules != rules || ruleCount != rules.size()) {
            forget(rules);
            graph = g.uid();
            this.rules = rules;
            ruleCount = rules.size();
        }
        create0 = createAt[ruleIx];
        recognition0 = recognitionAt[ruleIx];
        return this;
    }

    /** Leere Plätze für {@code rules}: ein Feld je Art für alle Pläne und
     *  alle Erkennungen, nicht eines je Regel (ein Regelsatz mit Tausenden
     *  Regeln, von denen jede ein- oder zweimal angewandt wird, zahlte für
     *  die Plätze mehr, als sie sparen). */
    private void forget(List<Rule> rules) {
        int n = rules.size();
        createAt = new int[n + 1];
        recognitionAt = new int[n + 1];
        int create = 0;
        int recognition = 0;
        for (int ri = 0; ri < n; ri++) {
            createAt[ri] = create;
            recognitionAt[ri] = recognition;
            Rule r = rules.get(ri);
            create += r.createNodes.size();
            recognition += r.corrRecognition.size();
        }
        createAt[n] = create;
        recognitionAt[n] = recognition;
        typ = none(create);
        attr = none(create);
        konst = none(create);
        attrCorrName = new String[create];
        attrCorrTyp = none(create);
        corrTyp = none(recognition);
        endpointTyp = none(recognition);
    }

    private static int[] none(int n) {
        int[] a = new int[n];
        Arrays.fill(a, NONE);
        return a;
    }

    /** Der Typ {@code name} in den Tabellen von {@code g}, beim ersten
     *  Mal eingetragen ({@link Graph#intern}). */
    private static int typOf(int[] kept, int at, Graph g, String name) {
        int t = kept[at];
        if (t == NONE) {
            t = g.intern(name);
            kept[at] = t;
        } else if (Engine.CHECKS) {
            check(name, t, g.lookup(name));
        }
        return t;
    }

    /** Der Typ {@code name}, wenn die Tabellen von {@code g} ihn kennen,
     *  nachgeschlagen, bis sie es tun. {@link #NONE}: noch nicht. */
    private static int knownTyp(int[] kept, int at, Graph g, String name) {
        int t = kept[at];
        if (t == NONE) {
            Integer l = g.lookup(name);
            if (l != null) {
                t = l;
                kept[at] = t;
            }
        } else if (Engine.CHECKS) {
            check(name, t, g.lookup(name));
        }
        return t;
    }

    private static void check(String name, int kept, Integer now) {
        if (now == null || now != kept) {
            throw new IllegalStateException("Names: für " + name + " ist " + kept
                    + " behalten, die Tabelle des Graphen nennt " + now);
        }
    }

    /** Der Korrespondenz-Knoten {@code id} der Planposition
     *  {@code planIx} ({@link Graph#addCorrAt}). */
    Id addCorr(Graph g, int planIx, Id id, String typName) {
        int t = typOf(typ, create0 + planIx, g, typName);
        g.insertNode(new Node(id, t, St.GHOST, null, null));
        return id;
    }

    /** Der Ghost-Knoten {@code id} der Planposition {@code planIx}
     *  ({@link Graph#addGhostAt}). */
    Id addGhost(Graph g, int planIx, Id id, String typName) {
        int t = typOf(typ, create0 + planIx, g, typName);
        g.insertNode(new Node(id, t, St.GHOST, null, null));
        return id;
    }

    /** Das abgeleitete Blatt {@code id} der Planposition {@code planIx}
     *  ({@link Graph#addDerivedLeafAt}). */
    Id addDerivedLeaf(Graph g, int planIx, Id id, String typName, Id source, Chain transform) {
        int t = typOf(typ, create0 + planIx, g, typName);
        g.insertNode(new Node(id, t, St.GHOST, source, transform));
        return id;
    }

    /** Das Konstanten-Blatt {@code id} der Planposition {@code planIx}
     *  ({@link Graph#addKonstLeafAt}): erst der Typ, dann die Konstante,
     *  wie dort. */
    Id addKonstLeaf(Graph g, int planIx, Id id, String typName, String value) {
        int at = create0 + planIx;
        int t = typOf(typ, at, g, typName);
        int k = konst[at];
        if (k == NONE) {
            k = g.internKonst(value);
            konst[at] = k;
        } else if (Engine.CHECKS) {
            int now = g.internKonst(value);
            if (now != k) {
                throw new IllegalStateException("Names: für die Konstante " + value + " ist " + k
                        + " behalten, die Tabelle des Graphen nennt " + now);
            }
        }
        g.insertNode(new Node(id, t, St.GHOST, null, null, k));
        return id;
    }

    /** Die Attribut-Korrespondenz des dynamischen Blatts der Planposition
     *  {@code planIx}: eine Korrespondenz vom Typ {@code corr_typ} an der
     *  Quelle des Blatts ({@link Graph#addCorr}). */
    Id addAttrCorr(Graph g, int planIx, Id source, String corr, String typName, Id[] bindings) {
        int at = create0 + planIx;
        String name = attrCorrName[at];
        if (name == null) {
            name = corr + "_" + typName;
            attrCorrName[at] = name;
        }
        int t = typOf(attrCorrTyp, at, g, name);
        Id id = Ident.identCorr(source, name, bindings);
        g.insertNode(new Node(id, t, St.GHOST, null, null));
        return id;
    }

    /** Das Quellblatt des dynamischen Blatts der Planposition
     *  {@code planIx}: das erste matchbare Kind von {@code anchor} mit dem
     *  Typ {@code attrName} ({@link Graph#childLeafOfType}). */
    Id childLeaf(Graph g, int planIx, Id anchor, String attrName) {
        int t = knownTyp(attr, create0 + planIx, g, attrName);
        Id found = null;
        if (t != NONE) {
            for (Part p : g.partsOfType(anchor, t)) {
                if (!p.outgoing) continue;
                Node n = g.node(p.other);
                if (n != null && n.status.matchable()) {
                    found = p.other;
                    break;
                }
            }
        }
        if (Engine.CHECKS) {
            Id want = g.childLeafOfType(anchor, attrName);
            if (!Objects.equals(want, found)) {
                throw new IllegalStateException("Names: Quellblatt " + attrName + " von "
                        + HexFormat.of().formatHex(anchor.b) + " weicht von childLeafOfType ab");
            }
        }
        return found;
    }

    /** Der Korrespondenztyp des Eintrags {@code ix} der
     *  {@code corrRecognition} der Regel, {@link #NONE}: der Graph kennt
     *  ihn nicht. */
    int corrTyp(Graph g, int ix, String typName) {
        return knownTyp(corrTyp, recognition0 + ix, g, typName);
    }

    /** Der Typ des Gegenendes des Eintrags {@code ix} der
     *  {@code corrRecognition} der Regel, {@link #NONE}: der Graph kennt
     *  ihn nicht. */
    int endpointTyp(Graph g, int ix, String typName) {
        return knownTyp(endpointTyp, recognition0 + ix, g, typName);
    }
}
