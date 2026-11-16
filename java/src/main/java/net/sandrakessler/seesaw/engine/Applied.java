package net.sandrakessler.seesaw.engine;

import java.util.Arrays;
import java.util.HashSet;

/**
 * Die angewandten Matches (Rust {@code Applied} in {@code applied_marks.rs}):
 * eine Marke je Nummer eines Matches im Match-Speicher, die der gezogene
 * Kandidat traegt ({@link Engine.TodoKey#mix}). 2.0.2 fuehrte die angewandten
 * Schluessel als Menge. Alle drei Zugriffe eines Schritts betreffen den Match
 * unter dem Schluessel in {@code byKey}, und der Kandidat traegt ihn:
 * <ul>
 * <li>{@link Engine#record} traegt einen Kandidaten nur ein, wenn unter seinem
 *     Schluessel kein Match steht, und legt seinen Match im selben Aufruf dort
 *     ab. Den Schluessel nehmen nur der Schritt, fuer den eben gezogenen
 *     Kandidaten, und {@link Engine#retractMatch} heraus. Steht darunter der
 *     zurueckgezogene Match, ist sein Kandidat danach nicht im Todo:
 *     retractMatch nimmt ihn heraus, oder er ging, als der Match starb. Steht
 *     darunter ein anderer Match derselben Ref-Folge, verlor der
 *     zurueckgezogene den Schluessel frueher, durch ein frueheres retractMatch
 *     (es erreicht auch tote Matches) oder als Kandidat mit tentativ
 *     zurueckgezogenem Anker, und der andere wurde danach eingetragen. In
 *     beiden Faellen stehen beide in denselben Listen von {@code byElement}:
 *     gleiche Refs, und record gibt einer Kante in beiden Zweigen die Id
 *     {@code identConnection(refs[from], refs[to])}. Der andere steht dort
 *     hinter dem zurueckgezogenen, und {@link Engine#drainRetraction} und
 *     {@link Engine#linkRemoved} lesen die Laenge vor der Schleife. Dieselbe
 *     Schleife zieht ihn vor dem naechsten Schritt zurueck. Ein Kandidat im
 *     Todo traegt also den Match unter seinem Schluessel.
 * <li>Vermerkt wird ein Schluessel, wenn sein Kandidat gezogen wird, vergessen,
 *     wenn retractMatch ihn aus {@code byKey} nimmt. Der Vermerk besteht also
 *     so lange, wie sein Match unter dem Schluessel steht: eine Marke am Match.
 * </ul>
 *
 * <p>Mit Pruefung ({@link Engine#CHECKS} oder vom Test gewaehlt) fuehrt
 * {@link #check} die Menge der angewandten Schluessel wie 2.0.2 daneben. Jede
 * Frage und jede Aenderung vergleicht, eine Abweichung wirft.
 *
 * <p>Ruft ein Host das oeffentliche retractMatch selbst fuer einen toten Match,
 * dessen Schluessel ein anderer haelt, gilt die Grundlage nicht. Unter Pruefung
 * wirft dann {@code pulledMatch}, sobald der Kandidat des anderen angewandt
 * wird. Wird er als Duplikat vermerkt, weicht erst eine spaetere Frage nach dem
 * Schluessel oder sein Vergessen ab, dort wirft die Pruefung. Mit tentativem
 * Anker oder am V7-Guard verhalten sich beide Wege gleich.
 */
final class Applied {
    /** Ein Bit je Match. */
    private long[] marked = new long[1];
    /** Zahl der Marken. */
    private int count;
    /** Pruefung: die angewandten Schluessel, wie 2.0.2 sie fuehrte. null
     *  ohne Pruefung. */
    final HashSet<MatchKey> check;

    Applied(boolean checks) {
        check = checks ? new HashSet<>() : null;
    }

    /** Traegt der Match {@code mix} die Marke? */
    boolean marked(int mix) {
        int word = mix >>> 6;
        return word < marked.length && (marked[word] & (1L << mix)) != 0;
    }

    /** Ist der gezogene Kandidat mit dem Schluessel {@code key}, der den
     *  Match {@code mix} traegt, schon angewandt? */
    boolean contains(MatchKey key, int mix) {
        boolean m = marked(mix);
        if (check != null && m != check.contains(key)) {
            throw new IllegalStateException("applied: die Marke des Matches " + mix + " (" + m
                    + ") weicht von der Menge der angewandten Schluessel ab, Regel " + key.ruleIx);
        }
        return m;
    }

    /** Vermerkt den gezogenen Kandidaten des Matches {@code mix} als
     *  angewandt. */
    void insert(MatchKey key, int mix) {
        int word = mix >>> 6;
        if (word >= marked.length) marked = Arrays.copyOf(marked, Math.max(word + 1, 2 * marked.length));
        long bit = 1L << mix;
        if ((marked[word] & bit) == 0) {
            marked[word] |= bit;
            count++;
        }
        if (check != null) {
            check.add(key);
            same();
        }
    }

    /**
     * Vergisst, dass {@code key} angewandt war (Rust {@code forget} und
     * {@code forget_match}). {@code current}: der Match, der unter dem
     * Schluessel stand, der also seine Marke traegt, null fuer keinen.
     */
    void forget(MatchKey key, Integer current) {
        if (check != null) check.remove(key);
        if (current != null) {
            int mix = current;
            int word = mix >>> 6;
            long bit = 1L << mix;
            if (word < marked.length && (marked[word] & bit) != 0) {
                marked[word] &= ~bit;
                count--;
            }
        }
        if (check != null) same();
    }

    /** Ist nichts angewandt? */
    boolean isEmpty() {
        return count == 0;
    }

    private void same() {
        if (count != check.size()) {
            throw new IllegalStateException("applied: " + count + " Marken, aber " + check.size()
                    + " angewandte Schluessel");
        }
    }
}
