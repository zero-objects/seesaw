package net.sandrakessler.seesaw.graph;

import java.util.Arrays;
import java.util.ConcurrentModificationException;
import java.util.Iterator;
import java.util.NoSuchElementException;
import java.util.TreeSet;

/**
 * Die Participation-Liste eines Knotens: eine Menge in der Ordnung von
 * {@link Part} (Rust {@code Parts}). Ein sortiertes Array, solange sie
 * kurz ist, eine {@code TreeSet} ab {@link #TREE_AT} Einträgen. Die
 * meisten Knoten haben wenige Participations, für die eine
 * {@code TreeSet} (ein Eintragsobjekt je Participation) den größten
 * Teil des Speichers ausmachte. Wenige Hubs haben viele, in deren Array
 * ein Einfügen zu viel verschieben würde.
 *
 * <p>Das Array beginnt mit zwei Plätzen: die meisten Knoten haben eine
 * oder zwei Participations (ein Blatt, eine Verbindung zum Besitzer).
 *
 * <p>Eine Liste in Baumform bleibt in Baumform, auch wenn {@link #remove}
 * sie wieder unter {@link #TREE_AT} Einträge bringt. Rust kennt kein
 * Entfernen, eine Liste wird dort nie kürzer.
 *
 * <p>Wer die Liste während eines Durchlaufs ändert, bekommt beim
 * nächsten Schritt eine {@code ConcurrentModificationException}, wie
 * bisher bei der {@code TreeSet}.
 */
public final class Parts implements Iterable<Part> {
    /** Hat das Array so viele Einträge, wird die Liste beim nächsten
     *  Einfügen zur {@code TreeSet} (Rust {@code PARTS_TREE_AT}). */
    public static final int TREE_AT = 48;

    /** Sortiert, belegt sind die ersten {@code n} Plätze. null vor dem
     *  ersten Eintrag und in Baumform. */
    private Part[] flat;
    private int n;
    /** Nicht null: die Liste ist in Baumform, {@code flat} und {@code n}
     *  gelten nicht. */
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
     * Array schon {@link #TREE_AT} Einträge, wird die Liste zur
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

    /** Entfernt {@code p}. {@code false}, wenn es nicht da war. Die Form
     *  (Array oder Baum) bleibt. */
    public boolean remove(Part p) {
        if (tree != null) {
            if (!tree.remove(p)) return false;
        } else {
            int at = find(p);
            if (at < 0) return false;
            System.arraycopy(flat, at + 1, flat, at, n - at - 1);
            flat[--n] = null;
        }
        mods++;
        return true;
    }

    /** Alle Einträge in Part-Ordnung. */
    @Override public Iterator<Part> iterator() {
        return tree != null ? tree.iterator() : new Run(this, 0, n);
    }

    /**
     * Die Einträge, deren Gegenende den Typ {@code typ} hat, in
     * Part-Ordnung (Rust {@code Parts::of_type}). Eine Sicht: jeder
     * Durchlauf sucht den Bereich in der Liste, wie sie dann ist. Im
     * Array über den Typ allein, er ist das erste Glied der Ordnung. In
     * Baumform zwischen den festen Grenzen aus {@link Part#lowerBound}.
     * Legt weder eine Liste noch eine {@code Id} an.
     */
    public Iterable<Part> ofType(int typ) {
        return new OfType(this, typ);
    }

    /** In Baumform? Für Tests. */
    boolean isTree() {
        return tree != null;
    }

    private int find(Part p) {
        return flat == null ? -1 : Arrays.binarySearch(flat, 0, n, p);
    }

    /** Erster Index in {@code a[from, to)}, dessen Gegentyp nicht kleiner
     *  als {@code typ} ist, mit {@code after} der erste, dessen Gegentyp
     *  größer ist. */
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

    /** Sicht auf den Bereich eines Gegentyps. */
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

    /** Durchlauf über {@code flat[from, to)} einer Liste in Arrayform. */
    private static final class Run implements Iterator<Part> {
        private final Parts owner;
        private final Part[] a;
        private final int to;
        private final int mods;
        private int i;

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
            return a[i++];
        }
    }
}
