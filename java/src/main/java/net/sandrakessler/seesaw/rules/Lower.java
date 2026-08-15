package net.sandrakessler.seesaw.rules;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.plan.CreateNode;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.PatNode;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Validate.ResolvedBinding;
import net.sandrakessler.seesaw.rules.Validate.ResolvedCorr;
import net.sandrakessler.seesaw.rules.Validate.ResolvedRule;
import net.sandrakessler.seesaw.rules.Validate.ResolvedSide;
import net.sandrakessler.seesaw.rules.Validate.Source;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

/**
 * Senkt validierte Regeln in gerichtete Erzeugungspläne, Spiegel von
 * {@code seesaw-core/src/rules/lower.rs}.
 *
 * <p>Eine Regel wird zu zwei Plänen: vorwärts matcht die linke Seite
 * und die rechte wird erzeugt, rückwärts gespiegelt.
 *
 * <p>Der Richtungssuffix am Regelnamen ist {@code →} (U+2192) und
 * {@code ←} (U+2190). Er geht in die Identität von Regel-Konstanten
 * ein, also müssen es genau diese Zeichen sein.
 */
public final class Lower {
    private Lower() {}

    /** Ref-Kodierung des Plans: gematcht {@code >= 0}, neu {@code -ix-1}. */
    private static int refNew(int ix) {
        return -ix - 1;
    }

    private static int refMatched(int pos) {
        return pos;
    }

    /** Alle Regeln, vorwärts und rückwärts abwechselnd. */
    public static List<Rule> lowerAll(Validate.Resolved res, Graph g) {
        List<Rule> out = new ArrayList<>(res.rules.size() * 2);
        for (Validate.ResolvedRule r : res.rules) {
            out.add(lowerDirected(r, g, res.chains, true));
            out.add(lowerDirected(r, g, res.chains, false));
        }
        return out;
    }

    /** Beide Richtungen einer einzelnen Regel. */
    public static List<Rule> lowerRule(Validate.Resolved res, int ix, Graph g) {
        ResolvedRule r = res.rules.get(ix);
        return List.of(lowerDirected(r, g, res.chains, true),
                lowerDirected(r, g, res.chains, false));
    }

    /** Quelle und Ziel einer Bindung in Laufrichtung. */
    private static Source src(ResolvedBinding b, boolean forward) {
        return forward ? b.left : b.right;
    }

    private static Source dst(ResolvedBinding b, boolean forward) {
        return forward ? b.right : b.left;
    }

    /**
     * Die Kette einer Bindung in Laufrichtung. Bindungen sind
     * links→rechts deklariert; rückwärts erzeugte Blätter tragen die
     * UMGEKEHRTE Kette, damit die Wertauflösung immer vorwärts bleibt.
     */
    private static Chain bindingChain(ResolvedBinding b, Validate.ChainTable chains,
            boolean forward) {
        Chain c = chains.chain(b.chain);
        return forward ? c : c.inverse();
    }

    /** (Eingangs-Anker, Ausgangs-Endpunkt) einer Corr in Laufrichtung. */
    private static int[] corrEnds(ResolvedCorr c, boolean forward) {
        return forward ? new int[] {c.left, c.right} : new int[] {c.right, c.left};
    }

    private static Rule lowerDirected(ResolvedRule rule, Graph g, Validate.ChainTable chains,
            boolean forward) {
        ResolvedSide inn = forward ? rule.left : rule.right;
        ResolvedSide out = forward ? rule.right : rule.left;

        Rule dr = new Rule();
        dr.patNodes = new ArrayList<>();
        dr.patLinks = new ArrayList<>();

        // Pattern: Eingangsseite.
        for (Validate.ResolvedNode ns : inn.nodes) {
            dr.patNodes.add(new PatNode(g.intern(ns.typ), ns.predicate));
        }
        for (int[] l : inn.links) {
            dr.patLinks.add(PatLink.directed(l[0], l[1]));
        }

        // Kontext der Ausgangsseite: Ausgangsposition -> Pattern-Position.
        Map<Integer, Integer> outCtx = new LinkedHashMap<>();
        // Same-Domain-Kontext: der rechte Knoten IST ein linker.
        for (int j = 0; j < rule.right.nodes.size(); j++) {
            int k = rule.right.nodes.get(j).sameAs;
            if (k >= 0) {
                if (forward) {
                    outCtx.put(j, k);
                } else {
                    outCtx.put(k, j);
                }
            }
        }
        // Kontext ohne Corr (context: true) der AUSGANGSSEITE: wird
        // gematcht, nie erzeugt. Kontextknoten der Eingangsseite sind
        // ohnehin gewoehnliche Pattern-Knoten.
        for (int j = 0; j < out.nodes.size(); j++) {
            Validate.ResolvedNode ns = out.nodes.get(j);
            if (ns.context && !outCtx.containsKey(j)) {
                int pos = dr.patNodes.size();
                dr.patNodes.add(new PatNode(g.intern(ns.typ), ns.predicate));
                outCtx.put(j, pos);
            }
        }
        // references-Corrs VOR der Ausgangs-Link-Schleife eintragen,
        // sonst fielen Ausgangs-Links an einem Referenz-Endpunkt still
        // als Pattern-Bedingung aus. Haengt nur Knoten hinten an, die
        // Eingangspositionen verschieben sich nicht (μ bleibt stabil).
        Set<Integer> refEndpoints = new LinkedHashSet<>();
        for (ResolvedCorr c : rule.corrs) {
            if (c.role != Format.Role.REFERENCES) {
                continue;
            }
            int[] ends = corrEnds(c, forward);
            int corrPat = dr.patNodes.size();
            dr.patNodes.add(new PatNode(g.intern(c.typ), null));
            dr.patLinks.add(PatLink.context(corrPat, ends[0]));
            int ctxPat = dr.patNodes.size();
            dr.patNodes.add(new PatNode(g.intern(out.nodes.get(ends[1]).typ), null));
            dr.patLinks.add(PatLink.context(corrPat, ctxPat));
            outCtx.put(ends[1], ctxPat);
            refEndpoints.add(ends[1]);
        }
        // Ausgangs-Links ZWISCHEN Kontextknoten werden Pattern-
        // Bedingungen. Ausnahme: eine Kante zwischen ZWEI
        // references-Endpunkten ist keine Vorbedingung, sondern das,
        // was die Regel herstellt.
        for (int[] l : out.links) {
            if (refEndpoints.contains(l[0]) && refEndpoints.contains(l[1])) {
                continue;
            }
            Integer pa = outCtx.get(l[0]);
            Integer pb = outCtx.get(l[1]);
            if (pa != null && pb != null) {
                dr.patLinks.add(PatLink.directed(pa, pb));
            }
        }
        // Wert-Gleichheiten: innerhalb der Eingangsseite direkt,
        // innerhalb der Ausgangsseite nur zwischen Kontextknoten,
        // seitenuebergreifend Eingang gegen Ausgangs-Kontext.
        for (int[] l : inn.sameValueLinks) {
            dr.patLinks.add(PatLink.sameValue(l[0], l[1]));
        }
        for (int[] l : out.sameValueLinks) {
            Integer pa = outCtx.get(l[0]);
            Integer pb = outCtx.get(l[1]);
            if (pa != null && pb != null) {
                dr.patLinks.add(PatLink.sameValue(pa, pb));
            }
        }
        for (int[] j : rule.joins) {
            // joins sind links→rechts deklariert; rueckwaerts ist der
            // rechte Knoten der Eingang.
            int inPos = forward ? j[0] : j[1];
            int outPos = forward ? j[1] : j[0];
            Integer pb = outCtx.get(outPos);
            if (pb != null) {
                dr.patLinks.add(PatLink.sameValue(inPos, pb));
            }
            // Join auf einem ERZEUGTEN Ausgangsknoten: keine Bedingung,
            // der Wert entsteht ueber Bindung oder Konstante.
        }

        // Erzeugungsplan.
        List<CreateNode> createNodes = new ArrayList<>();
        List<int[]> createLinks = new ArrayList<>();

        List<ResolvedCorr> establishes = new ArrayList<>();
        for (ResolvedCorr c : rule.corrs) {
            if (c.role == Format.Role.ESTABLISHES) {
                establishes.add(c);
            }
        }
        // Leere Kette fuer die Paar-Identitaet der Corr: sie traegt
        // keinen Wert, nur den Ref des Gegenstuecks.
        Chain identChain = Chain.IDENTITY;
        // (Eingangs-Anker, etablierter Ausgang, Planindex) je Corr.
        List<int[]> estCorrs = new ArrayList<>();
        // Ausgangsposition -> Planindex der Corr, die sie etabliert.
        Map<Integer, Integer> corrOfOut = new LinkedHashMap<>();
        for (ResolvedCorr c : establishes) {
            int[] ends = corrEnds(c, forward);
            int inAnchor = ends[0];
            int estOut = ends[1];
            // Paar-Identitaet: ist das etablierte Gegenstueck selbst
            // gematcht (Kontext oder Same-Domain), geht sein Ref in die
            // Corr-Identitaet ein, sonst fielen mehrere Matches am
            // selben Anker zusammen.
            Integer estMatched = outCtx.get(estOut);
            int ix = createNodes.size();
            createNodes.add(new CreateNode(c.typ, refMatched(inAnchor),
                    estMatched == null ? -1 : estMatched,
                    estMatched == null ? null : identChain, true));
            createLinks.add(new int[] {refMatched(inAnchor), refNew(ix)});
            estCorrs.add(new int[] {inAnchor, estOut, ix});
            corrOfOut.putIfAbsent(estOut, ix);
        }
        Integer corrNewIx = estCorrs.isEmpty() ? null : estCorrs.get(0)[2];

        // Struktureller Eltern-Knoten je Ausgangsposition, aus den
        // Ausgangs-Links: das Ziel einer Kante haengt an ihrer Quelle.
        int[] outParent = new int[out.nodes.size()];
        java.util.Arrays.fill(outParent, -1);
        for (int[] l : out.links) {
            if (outParent[l[1]] < 0) {
                outParent[l[1]] = l[0];
            }
        }
        int[] outNewIx = new int[out.nodes.size()];
        java.util.Arrays.fill(outNewIx, -1);
        int firstCreated = -1;
        for (int i = 0; i < out.nodes.size(); i++) {
            if (outCtx.containsKey(i)) {
                continue; // Kontext, existiert schon, wird gematcht.
            }
            // Blatt-Bindungen: als abgeleitetes Blatt aus dem
            // Eingangsblatt erzeugt, gesucht ueber ALLE
            // establishes-Corrs.
            ResolvedBinding hit = null;
            String attrCorrTyp = null;
            for (ResolvedCorr c : establishes) {
                for (ResolvedBinding b : c.bindings) {
                    Source d = dst(b, forward);
                    if (d.isNode() && d.node == i) {
                        hit = b;
                        attrCorrTyp = c.typ;
                        break;
                    }
                }
                if (hit != null) {
                    break;
                }
            }
            int derivedLeaf = -1;
            Chain derivedTransform = null;
            if (hit != null) {
                Source s = src(hit, forward);
                if (!s.isNode()) {
                    throw new LowerException(rule.name
                            + ": binding on output node " + i
                            + " mixes a node and a type source");
                }
                derivedLeaf = s.node;
                derivedTransform = bindingChain(hit, chains, forward);
            }
            Validate.ResolvedNode ns = out.nodes.get(i);
            if (ns.constant != null && derivedLeaf >= 0) {
                throw new LowerException(rule.name
                        + ": output node " + i + " has both a constant and a binding");
            }
            String konst = ns.constant;
            int ix = createNodes.size();
            outNewIx[i] = ix;
            if (firstCreated < 0) {
                firstCreated = ix;
            }
            // Identitaets-Eltern: die Corr, die GENAU DIESE Position
            // etabliert, sonst der erzeugte strukturelle Eltern-Knoten,
            // sonst die erste Corr, sonst der Eingangs-Anker.
            // Kontext-Eltern taugen nicht als Identitaets-Eltern
            // (Geschwister-Kollision).
            int parent;
            Integer cix = corrOfOut.get(i);
            if (cix != null) {
                parent = refNew(cix);
            } else if (outParent[i] >= 0 && outNewIx[outParent[i]] >= 0) {
                parent = refNew(outNewIx[outParent[i]]);
            } else if (corrNewIx != null) {
                parent = refNew(corrNewIx);
            } else {
                parent = refMatched(inn.anchor);
            }
            createNodes.add(new CreateNode(ns.typ, parent, derivedLeaf, derivedTransform, konst));
            // Attribut-Korrespondenz: das Blattpaar bekommt eine EIGENE
            // Korrespondenz. Was das Regelformat `bindings` nennt, ist
            // im TGG-Sinn ein Attribut-Constraint zwischen zwei
            // Blaettern, also selbst eine Korrespondenz auf Blatt-Ebene
            // (Sandra 2026-08-12), mit derselben Identitaetsableitung
            // wie jede andere. Spiegel von lower.rs.
            if (attrCorrTyp != null && derivedLeaf >= 0) {
                int acix = createNodes.size();
                createNodes.add(new CreateNode(attrCorrTyp + "_" + ns.typ,
                        refMatched(derivedLeaf), -1, null, null, -1, null, null, false, true));
                createLinks.add(new int[] {refNew(acix), refMatched(derivedLeaf)});
                createLinks.add(new int[] {refNew(acix), refNew(ix)});
            }
        }
        // DYNAMISCHE Bindungen (Blatt-Typ statt Blatt-Position): ein
        // Blatt je Bindung am etablierten Endpunkt SEINER Corr, die
        // Quelle wird beim Anwenden am Eingangs-Anker gesucht
        // (apply-if-present).
        for (int ci = 0; ci < establishes.size(); ci++) {
            ResolvedCorr c = establishes.get(ci);
            int inAnchor = estCorrs.get(ci)[0];
            int estOut = estCorrs.get(ci)[1];
            // Ziel-Eltern: erzeugter Knoten ODER Kontext (Attribute
            // werden auch an Bestehenden gesetzt).
            Integer estRef = null;
            if (outNewIx[estOut] >= 0) {
                estRef = refNew(outNewIx[estOut]);
            } else if (outCtx.containsKey(estOut)) {
                estRef = refMatched(outCtx.get(estOut));
            }
            for (ResolvedBinding b : c.bindings) {
                Source s = src(b, forward);
                Source d = dst(b, forward);
                if (s.isNode() && d.isNode()) {
                    continue; // statische Bindung, oben behandelt
                }
                if (s.isNode() != d.isNode()) {
                    throw new LowerException(rule.name + ": corr " + c.typ
                            + " mixes a node and a type source in one binding");
                }
                if (estRef == null) {
                    throw new LowerException(rule.name
                            + ": dynamic binding — established node neither created nor context");
                }
                Chain t = bindingChain(b, chains, forward);
                int ix = createNodes.size();
                // Der dynamische Fall bekommt seine Blatt-Korrespondenz
                // beim Anwenden, weil die Quelle erst dort feststeht.
                createNodes.add(new CreateNode(d.leafType, estRef, -1, null, null,
                        inAnchor, s.leafType, t, false, false, c.typ));
                createLinks.add(new int[] {estRef, refNew(ix)});
            }
        }
        for (int[] l : out.links) {
            createLinks.add(new int[] {planRef(rule, out, outNewIx, outCtx, l[0]),
                    planRef(rule, out, outNewIx, outCtx, l[1])});
        }
        // Corr → etablierter Endpunkt (Provenienzkette), je Corr.
        for (int[] e : estCorrs) {
            int estOut = e[1];
            int cix = e[2];
            Integer target = null;
            if (outNewIx[estOut] >= 0) {
                target = refNew(outNewIx[estOut]);
            } else if (outCtx.containsKey(estOut)) {
                target = refMatched(outCtx.get(estOut));
            } else if (firstCreated >= 0) {
                target = refNew(firstCreated);
            }
            if (target != null) {
                createLinks.add(new int[] {refNew(cix), target});
            }
        }

        dr.name = rule.name + (forward ? "→" : "←");
        dr.rank = rule.rank;
        dr.direction = establishes.isEmpty()
                ? Rule.Direction.UNDIRECTED
                : (forward ? Rule.Direction.FORWARD : Rule.Direction.BACKWARD);
        dr.createNodes = createNodes;
        dr.createLinks = createLinks;
        // Delta-Routing nur ueber die Eingangsanker etablierender
        // Korrespondenzen. References sind Kontext und waehlen keine
        // Richtung. Nach der Auswahl bleibt die gesamte gerichtete
        // Regelfamilie aktiv, damit neuer Kontext Folgeregeln weckt.
        Set<String> types = new TreeSet<>();
        for (ResolvedCorr c : establishes) {
            int[] ends = corrEnds(c, forward);
            types.add(inn.nodes.get(ends[0]).typ);
        }
        dr.inputTypes = new ArrayList<>(types);
        dr.corrRecognition = new ArrayList<>();
        for (ResolvedCorr c : establishes) {
            int[] ends = corrEnds(c, forward);
            dr.corrRecognition.add(
                    new Object[] {c.typ, ends[0], out.nodes.get(ends[1]).typ});
        }
        return dr;
    }

    /** Ref eines Ausgangsknotens im Plan: erzeugt oder Kontext. */
    private static int planRef(ResolvedRule rule, ResolvedSide out, int[] outNewIx,
            Map<Integer, Integer> outCtx, int i) {
        if (outNewIx[i] >= 0) {
            return refNew(outNewIx[i]);
        }
        Integer pat = outCtx.get(i);
        if (pat != null) {
            return refMatched(pat);
        }
        throw new LowerException(rule.name
                + ": output link endpoint " + i + " neither created nor context");
    }

    /** Eine Regel liess sich nicht senken. */
    public static final class LowerException extends RuntimeException {
        private static final long serialVersionUID = 1L;

        LowerException(String message) {
            super(message);
        }
    }
}
