package net.sandrakessler.seesaw.rules;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;

/**
 * Validierung einer Regeldatei, Spiegel von
 * {@code seesaw-core/src/rules/validate.rs}.
 *
 * <p>Aus benannten Verweisen werden Positionen, und jeder Verweis, der
 * ins Leere zeigt, wird zum Ladefehler mit Fundstelle. Was hier
 * durchkommt, kann {@link Lower} ohne weitere Prüfung senken.
 */
public final class Validate {
    private Validate() {}

    // ── Ergebnis ──

    /** Ein Regelsatz mit aufgelösten Verweisen. */
    public static final class Resolved {
        public final String name;
        public final List<ResolvedRule> rules;
        /** Interning-Tabelle der Transformationsketten. */
        public final ChainTable chains;

        Resolved(String name, List<ResolvedRule> rules, ChainTable chains) {
            this.name = name;
            this.rules = rules;
            this.chains = chains;
        }
    }

    public static final class ResolvedRule {
        public final String name;
        public final long rank;
        public final ResolvedSide left;
        public final ResolvedSide right;
        public final List<ResolvedCorr> corrs;
        public final List<int[]> joins;

        ResolvedRule(String name, long rank, ResolvedSide left, ResolvedSide right,
                List<ResolvedCorr> corrs, List<int[]> joins) {
            this.name = name;
            this.rank = rank;
            this.left = left;
            this.right = right;
            this.corrs = corrs;
            this.joins = joins;
        }
    }

    public static final class ResolvedSide {
        public final int anchor;
        public final List<ResolvedNode> nodes;
        public final List<int[]> links;
        public final List<int[]> sameValueLinks;

        ResolvedSide(int anchor, List<ResolvedNode> nodes, List<int[]> links,
                List<int[]> sameValueLinks) {
            this.anchor = anchor;
            this.nodes = nodes;
            this.links = links;
            this.sameValueLinks = sameValueLinks;
        }
    }

    public static final class ResolvedNode {
        public final String name;
        public final String typ;
        public final Predicate predicate;
        public final boolean context;
        /** Position auf der linken Seite, -1 = kein same_as. */
        public final int sameAs;
        public final String constant;

        ResolvedNode(String name, String typ, Predicate predicate, boolean context,
                int sameAs, String constant) {
            this.name = name;
            this.typ = typ;
            this.predicate = predicate;
            this.context = context;
            this.sameAs = sameAs;
            this.constant = constant;
        }
    }

    public static final class ResolvedCorr {
        public final String typ;
        public final int left;
        public final int right;
        public final Format.Role role;
        public final List<ResolvedBinding> bindings;

        ResolvedCorr(String typ, int left, int right, Format.Role role,
                List<ResolvedBinding> bindings) {
            this.typ = typ;
            this.left = left;
            this.right = right;
            this.role = role;
            this.bindings = bindings;
        }
    }

    /**
     * Eine Quelle einer Bindung: entweder eine Knotenposition
     * (statisch) oder ein Blatt-Typname (dynamisch, am Anker gesucht).
     */
    public static final class Source {
        /** Knotenposition, oder -1 wenn dynamisch. */
        public final int node;
        /** Blatt-Typname, oder null wenn statisch. */
        public final String leafType;

        private Source(int node, String leafType) {
            this.node = node;
            this.leafType = leafType;
        }

        static Source ofNode(int pos) {
            return new Source(pos, null);
        }

        static Source ofLeafType(String typ) {
            return new Source(-1, typ);
        }

        public boolean isNode() {
            return leafType == null;
        }
    }

    public static final class ResolvedBinding {
        public final Source left;
        public final Source right;
        public final int chain;

        ResolvedBinding(Source left, Source right, int chain) {
            this.left = left;
            this.right = right;
            this.chain = chain;
        }
    }

    /** Interning der Ketten: gleiche Normalform, gleiche Kennung. */
    public static final class ChainTable {
        private final Map<Chain, Integer> byChain = new HashMap<>();
        private final List<Chain> chains = new ArrayList<>();

        public int intern(Chain c) {
            Integer id = byChain.get(c);
            if (id != null) {
                return id;
            }
            int next = chains.size();
            chains.add(c);
            byChain.put(c, next);
            return next;
        }

        public Chain chain(int id) {
            return chains.get(id);
        }
    }

    // ── Validierung ──

    public static Resolved validate(Format.RuleFile file) {
        if (file.format != Format.FORMAT_VERSION) {
            throw LoadException.version(file.format, Format.FORMAT_VERSION);
        }
        // Regelnamen muessen im Satz eindeutig sein: der Name geht in
        // die Identitaet von Regel-Konstanten ein. Vor der eigentlichen
        // Aufloesung geprueft, damit ein Namenskonflikt nicht erst
        // mitten in einer Regel auftaucht.
        Set<String> seen = new HashSet<>();
        for (Format.RuleDecl r : file.rules) {
            if (!seen.add(r.name)) {
                throw LoadException.duplicateRuleName(r.name);
            }
        }
        ChainTable chains = new ChainTable();
        List<ResolvedRule> rules = new ArrayList<>(file.rules.size());
        for (Format.RuleDecl r : file.rules) {
            rules.add(validateRule(r, chains));
        }
        return new Resolved(file.name, rules, chains);
    }

    private static ResolvedRule validateRule(Format.RuleDecl r, ChainTable chains) {
        Map<String, Integer> leftIx = sideIndex(r.left, r.name, "left");
        ResolvedSide left = resolveSide(r.left, leftIx, r.name, "left", null);
        Map<String, Integer> rightIx = sideIndex(r.right, r.name, "right");
        ResolvedSide right = resolveSide(r.right, rightIx, r.name, "right", leftIx);

        List<ResolvedCorr> corrs = new ArrayList<>(r.corrs.size());
        for (Format.CorrDecl c : r.corrs) {
            int l = indexOf(leftIx, c.left, r.name, "left");
            int rr = indexOf(rightIx, c.right, r.name, "right");
            List<ResolvedBinding> bindings = new ArrayList<>(c.bindings.size());
            for (Format.BindingDecl b : c.bindings) {
                bindings.add(resolveBinding(b, r.name, c.typ, leftIx, rightIx, chains));
            }
            corrs.add(new ResolvedCorr(c.typ, l, rr, c.role, bindings));
        }
        List<int[]> joins = new ArrayList<>(r.joins.size());
        for (String[] j : r.joins) {
            joins.add(new int[] {
                indexOf(leftIx, j[0], r.name, "left"),
                indexOf(rightIx, j[1], r.name, "right"),
            });
        }
        ResolvedRule rule = new ResolvedRule(r.name, r.rank, left, right, corrs, joins);
        checkValueRoles(rule);
        return rule;
    }

    private static int indexOf(Map<String, Integer> map, String name, String rule, String side) {
        Integer i = map.get(name);
        if (i == null) {
            throw LoadException.unknownNode(rule, side, name);
        }
        return i;
    }

    private static Map<String, Integer> sideIndex(Format.SideDecl side, String rule, String tag) {
        Map<String, Integer> map = new HashMap<>();
        for (int i = 0; i < side.nodes.size(); i++) {
            if (map.put(side.nodes.get(i).name, i) != null) {
                throw LoadException.duplicateNode(rule, tag, side.nodes.get(i).name);
            }
        }
        return map;
    }

    private static ResolvedSide resolveSide(Format.SideDecl side, Map<String, Integer> index,
            String rule, String tag, Map<String, Integer> leftIndex) {
        Integer anchor = index.get(side.anchor);
        if (anchor == null) {
            throw LoadException.unknownAnchor(rule, tag, side.anchor);
        }
        List<int[]> links = resolvePairs(side.links, index, rule, tag);
        checkNoDuplicatePairs(links, side, rule, tag, false);
        List<int[]> sameValue = resolvePairs(side.sameValueLinks, index, rule, tag);
        checkNoDuplicatePairs(sameValue, side, rule, tag, true);

        List<ResolvedNode> nodes = new ArrayList<>(side.nodes.size());
        for (Format.NodeDecl n : side.nodes) {
            nodes.add(new ResolvedNode(n.name, n.typ, n.predicate, n.context,
                    resolveSameAs(n, rule, leftIndex), n.constant));
        }
        return new ResolvedSide(anchor, nodes, links, sameValue);
    }

    private static List<int[]> resolvePairs(List<String[]> pairs, Map<String, Integer> index,
            String rule, String tag) {
        List<int[]> out = new ArrayList<>(pairs.size());
        for (String[] p : pairs) {
            out.add(new int[] {
                indexOf(index, p[0], rule, tag),
                indexOf(index, p[1], rule, tag),
            });
        }
        return out;
    }

    private static void checkNoDuplicatePairs(List<int[]> pairs, Format.SideDecl side,
            String rule, String tag, boolean sameValue) {
        Set<Long> seen = new HashSet<>();
        for (int[] p : pairs) {
            long key = ((long) p[0] << 32) | (p[1] & 0xffffffffL);
            if (!seen.add(key)) {
                String a = side.nodes.get(p[0]).name;
                String b = side.nodes.get(p[1]).name;
                throw sameValue
                        ? LoadException.duplicateSameValueLink(rule, tag, a, b)
                        : LoadException.duplicateLink(rule, tag, a, b);
            }
        }
    }

    private static int resolveSameAs(Format.NodeDecl n, String rule,
            Map<String, Integer> leftIndex) {
        if (n.sameAs == null) {
            return -1;
        }
        if (leftIndex == null) {
            throw LoadException.sameAsOnLeft(rule, n.name);
        }
        Integer i = leftIndex.get(n.sameAs);
        if (i == null) {
            throw LoadException.unknownSameAs(rule, n.sameAs);
        }
        return i;
    }

    private static ResolvedBinding resolveBinding(Format.BindingDecl b, String rule, String corr,
            Map<String, Integer> leftIx, Map<String, Integer> rightIx, ChainTable chains) {
        Source left = pickSource(b.left, b.leftType, rule, corr, leftIx, "left");
        Source right = pickSource(b.right, b.rightType, rule, corr, rightIx, "right");
        if (left.isNode() != right.isNode()) {
            throw LoadException.mixedBinding(rule, corr);
        }
        return new ResolvedBinding(left, right, chains.intern(b.transform));
    }

    private static Source pickSource(String node, String leafType, String rule, String corr,
            Map<String, Integer> index, String side) {
        if (node != null && leafType != null) {
            throw LoadException.ambiguousBinding(rule, corr);
        }
        if (node == null && leafType == null) {
            throw LoadException.emptyBinding(rule, corr);
        }
        return node != null
                ? Source.ofNode(indexOf(index, node, rule, side))
                : Source.ofLeafType(leafType);
    }

    /**
     * Wertbedingung und Konstante muessen zur Rolle des Knotens passen.
     *
     * <p>Auf einem Knoten, den das Lowering ERZEUGT, wird eine
     * Wertbedingung nie gelesen. Genau eine Form ist zugelassen: eine
     * Gleichheit, deren Wert mit der Konstanten desselben Knotens
     * uebereinstimmt. Umgekehrt faellt eine Konstante auf einem Knoten,
     * der nie erzeugt wird, in beiden Richtungen durch.
     */
    private static void checkValueRoles(ResolvedRule rule) {
        for (boolean isLeft : new boolean[] {true, false}) {
            ResolvedSide side = isLeft ? rule.left : rule.right;
            String tag = isLeft ? "left" : "right";
            for (int i = 0; i < side.nodes.size(); i++) {
                ResolvedNode n = side.nodes.get(i);
                boolean created = isCreated(rule, isLeft, i);
                if (created && n.predicate != null) {
                    boolean equalsPred = n.predicate instanceof Predicate.Equals;
                    if (equalsPred && n.constant != null) {
                        String want = ((Predicate.Equals) n.predicate).expected();
                        if (!want.equals(n.constant)) {
                            throw LoadException.constantPredicateMismatch(rule.name, tag, n.name);
                        }
                    } else {
                        throw LoadException.predicateOnCreatedNode(rule.name, tag, n.name);
                    }
                }
                if (!created && n.constant != null) {
                    throw LoadException.constantOnMatchedNode(rule.name, tag, n.name);
                }
            }
        }
    }

    /** Erzeugt das Lowering diesen Knoten in einer der Richtungen? */
    private static boolean isCreated(ResolvedRule rule, boolean isLeft, int i) {
        ResolvedSide side = isLeft ? rule.left : rule.right;
        if (side.nodes.get(i).context) {
            return false;
        }
        boolean sameAsPartner;
        if (isLeft) {
            sameAsPartner = rule.right.nodes.stream().anyMatch(n -> n.sameAs == i);
        } else {
            sameAsPartner = side.nodes.get(i).sameAs >= 0;
        }
        if (sameAsPartner) {
            return false;
        }
        for (ResolvedCorr c : rule.corrs) {
            if (c.role == Format.Role.REFERENCES && i == (isLeft ? c.left : c.right)) {
                return false;
            }
        }
        return true;
    }
}
