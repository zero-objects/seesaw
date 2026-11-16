package net.sandrakessler.seesaw.graph;

import java.util.Arrays;
import java.util.ConcurrentModificationException;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.TreeSet;

import net.sandrakessler.seesaw.ident.Id;

/**
 * Die Beteiligungsliste eines Knotens: eine Menge in der Ordnung von
 * {@link Part} (Rust {@code Parts}). Ein sortiertes Array bis
 * {@link #TREE_AT} Einträge, mit dem nächsten Eintrag wird sie eine
 * {@code TreeSet}. Die meisten Knoten haben wenige Participations, für
 * die eine {@code TreeSet} (ein Objekt je Eintrag) den größten Teil des
 * Speichers ausmachte. Wenige Hubs haben viele, in deren Array ein
 * Einfügen zu viel verschieben würde.
 *
 * <p>Das Array beginnt mit zwei Plätzen: die meisten Knoten haben eine
 * oder zwei Participations (ein Blatt, eine Verbindung zum Besitzer).
 *
 * <p>Eine Liste, die eine {@code TreeSet} ist, bleibt es, auch wenn
 * {@link #remove} sie wieder auf {@link #TREE_AT} oder weniger Einträge
 * bringt. Rust kennt kein Entfernen, eine Liste wird dort nie kürzer.
 *
 * <p>Wer die Liste während eines Durchlaufs ändert (außer über
 * {@code Iterator.remove} desselben Durchlaufs), bekommt beim nächsten
 * Schritt eine {@code ConcurrentModificationException}, wie bisher bei
 * der {@code TreeSet}.
 */
public final class Parts implements Iterable<Part> {
    /** Hat das Array so viele Einträge, wird die Liste beim nächsten
     *  Einfügen eine {@code TreeSet} (Rust {@code PARTS_TREE_AT}). */
    public static final int TREE_AT = 48;

    /** Sortiert, belegt sind die ersten {@code n} Plätze. null vor dem
     *  ersten Eintrag und solange {@code tree} gilt. */
    private Part[] flat;
    private int n;
    /** Nicht null: die Liste ist diese {@code TreeSet}, {@code flat} und
     *  {@code n} gelten nicht. */
    private TreeSet<Part> tree;
    /** Zahl der Änderungen, gegen sie prüft ein Durchlauf des Arrays. */
    private int mods;

    public int size() {
        return tree != null ? tree.size() : n;
    }

    public boolean isEmpty() {
        return size() == 0;
    }

    public boolean contains(Part p) {
        return tree != null ? tree.contains(p) : find(p) >= 0;
    }

    /**
     * Fügt {@code p} ein. {@code false}, wenn eine gleiche Participation
     * schon da ist (eine Menge wie bisher die {@code TreeSet}). Hat das
     * Array schon {@link #TREE_AT} Einträge, wird die Liste eine
     * {@code TreeSet}.
     */
    public boolean add(Part p) {
        if (tree != null) {
            if (!tree.add(p)) return false;
        } else {
            int at = find(p);
            if (at >= 0) return false;
            at = -at - 1;
            if (n >= TREE_AT) {
                TreeSet<Part> t = new TreeSet<>();
                for (int i = 0; i < n; i++) t.add(flat[i]);
                t.add(p);
                tree = t;
                flat = null;
                n = 0;
            } else {
                if (flat == null) {
                    flat = new Part[2];
                } else if (n == flat.length) {
                    flat = Arrays.copyOf(flat, Math.min(2 * n, TREE_AT));
                }
                System.arraycopy(flat, at, flat, at + 1, n - at);
                flat[at] = p;
                n++;
            }
        }
        mods++;
        return true;
    }

    /** Entfernt {@code p}. {@code false}, wenn es nicht da war. Ein
     *  Array bleibt ein Array, eine {@code TreeSet} eine {@code TreeSet}. */
    public boolean remove(Part p) {
        if (tree != null) {
            if (!tree.remove(p)) return false;
            mods++;
        } else {
            int at = find(p);
            if (at < 0) return false;
            removeAt(at);
        }
        return true;
    }

    /** Entfernt den Eintrag {@code flat[at]}. */
    private void removeAt(int at) {
        System.arraycopy(flat, at + 1, flat, at, n - at - 1);
        flat[--n] = null;
        mods++;
    }

    /** Alle Einträge in der Ordnung von Part. {@code Iterator.remove} entfernt
     *  den zuletzt gelieferten Eintrag, wie bisher bei der {@code TreeSet}. */
    @Override public Iterator<Part> iterator() {
        return tree != null ? tree.iterator() : new Run(this, 0, n);
    }

    /**
     * Die Einträge, deren anderes Ende den Typ {@code typ} hat
     * ({@code otherTyp == typ}), in der Ordnung von Part (Rust
     * {@code Parts::of_type}). Eine Sicht: jeder Durchlauf sucht den
     * Bereich in der Liste, wie sie dann ist. Im Array über
     * {@code otherTyp} allein, es ist das erste Glied der Ordnung. In der
     * {@code TreeSet} zwischen den festen Grenzen aus
     * {@link Part#lowerBound}. Legt weder eine Liste noch eine {@code Id}
     * an. {@code Iterator.remove} entfernt den zuletzt gelieferten Eintrag.
     */
    public Iterable<Part> ofType(int typ) {
        return new OfType(this, typ);
    }

    /**
     * Der erste Eintrag in der Ordnung von Part mit diesem
     * {@code otherTyp}, {@code other} und {@code outgoing}, gleich welche
     * Verbindung, null, wenn es keinen gibt. Die Verbindung
     * source→target steht in der Liste von source als (Typ von target,
     * target, ausgehend). Im Array eine binäre Suche ohne neues Objekt.
     */
    public Part first(int otherTyp, Id other, boolean outgoing) {
        if (tree != null) {
            Part p = tree.ceiling(new Part(otherTyp, other, outgoing, ZERO));
            return p != null && p.otherTyp == otherTyp && p.outgoing == outgoing
                    && p.other.equals(other) ? p : null;
        }
        int lo = 0, hi = n;
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            Part p = flat[mid];
            int c = Integer.compare(p.otherTyp, otherTyp);
            if (c == 0) c = p.other.compareTo(other);
            if (c == 0) c = Boolean.compare(p.outgoing, outgoing);
            if (c < 0) lo = mid + 1;
            else hi = mid;
        }
        if (lo == n) return null;
        Part p = flat[lo];
        return p.otherTyp == otherTyp && p.outgoing == outgoing && p.other.equals(other) ? p : null;
    }

    /** Die kleinste Id, untere Grenze der Verbindung in {@link #first}. */
    private static final Id ZERO = new Id(new byte[32]);

    /** Ist die Liste eine {@code TreeSet}? Für Tests. */
    boolean isTree() {
        return tree != null;
    }

    private int find(Part p) {
        return flat == null ? -1 : Arrays.binarySearch(flat, 0, n, p);
    }

    /** Erster Index in {@code a[from, to)}, dessen {@code otherTyp} nicht
     *  kleiner als {@code typ} ist, mit {@code after} der erste, dessen
     *  {@code otherTyp} größer ist. */
    private static int typeBound(Part[] a, int from, int to, int typ, boolean after) {
        int lo = from, hi = to;
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            int t = a[mid].otherTyp;
            if (t < typ || (after && t == typ)) lo = mid + 1;
            else hi = mid;
        }
        return lo;
    }

    /** Sicht auf den Bereich {@code otherTyp == typ}. */
    private static final class OfType implements Iterable<Part> {
        private final Parts owner;
        private final int typ;

        OfType(Parts owner, int typ) {
            this.owner = owner;
            this.typ = typ;
        }

        @Override public Iterator<Part> iterator() {
            Parts o = owner;
            if (o.tree != null) {
                Part lo = Part.lowerBound(typ);
                // Über Integer.MAX_VALUE gibt es keinen Typ, der den Bereich oben begrenzt.
                if (typ == Integer.MAX_VALUE) return o.tree.tailSet(lo, true).iterator();
                return o.tree.subSet(lo, true, Part.lowerBound(typ + 1), false).iterator();
            }
            // Ohne Einträge (flat == null) liest typeBound das Array nicht.
            int from = typeBound(o.flat, 0, o.n, typ, false);
            int to = typeBound(o.flat, from, o.n, typ, true);
            return new Run(o, from, to);
        }
    }

    /** Durchlauf über {@code flat[from, to)} einer Liste, die ein Array ist. */
    private static final class Run implements Iterator<Part> {
        private final Parts owner;
        private final Part[] a;
        private int to;
        private int mods;
        private int i;
        /** Index des zuletzt gelieferten Eintrags, -1: keiner oder schon entfernt. */
        private int last = -1;

        Run(Parts owner, int from, int to) {
            this.owner = owner;
            this.a = owner.flat;
            this.i = from;
            this.to = to;
            this.mods = owner.mods;
        }

        @Override public boolean hasNext() {
            return i < to;
        }

        @Override public Part next() {
            if (owner.mods != mods) throw new ConcurrentModificationException();
            if (i >= to) throw new NoSuchElementException();
            last = i;
            return a[i++];
        }

        /** Entfernt den zuletzt gelieferten Eintrag. Das Array bleibt
         *  dasselbe, der Durchlauf geht beim folgenden Eintrag weiter. */
        @Override public void remove() {
            if (last < 0) throw new IllegalStateException();
            if (owner.mods != mods) throw new ConcurrentModificationException();
            owner.removeAt(last);
            i = last;
            to--;
            last = -1;
            mods = owner.mods;
        }
    }
}
