package net.sandrakessler.seesaw.engine;

import java.util.Arrays;
import java.util.HashSet;

/**
 * Die angewandten Matches (Rust {@code Applied} in
 * {@code applied_marks.rs}): eine Marke je Nummer eines Matches im
 * Match-Speicher. {@link Engine#stepWithLimit} fragt und markiert ueber
 * die Nummer, die der gezogene Kandidat traegt
 * ({@link Engine.TodoKey#mix}), nichts ueber den Schluessel.
 *
 * <p>2.0.2 fuehrte die angewandten Schluessel (Regelindex, Ref-Folge) als
 * Menge. Alle drei Zugriffe eines Schritts betreffen den Match, der unter
 * dem Schluessel in {@code byKey} steht, und der Kandidat traegt diesen
 * Match:
 * <ul>
 * <li>{@link Engine#record} traegt einen Kandidaten nur ein, wenn unter
 *     seinem Schluessel kein Match steht, und legt seinen Match im selben
 *     Aufruf dort ab. Aus {@code byKey} nehmen den Schluessel nur der
 *     Schritt, fuer den Kandidaten, den er gerade gezogen hat, und
 *     {@link Engine#retractMatch}. Steht dort der zurueckgezogene Match
 *     selbst, ist sein Kandidat danach nicht im Todo: retractMatch nimmt
 *     ihn heraus, oder er ging schon, als der Match starb. Steht dort ein
 *     anderer Match mit derselben Ref-Folge (der zurueckgezogene wurde
 *     mit tentativ zurueckgezogenem Anker gezogen und vergessen, der
 *     andere danach eingetragen), dann steht dieser in {@code byElement}
 *     unter denselben Elementen, und dieselbe Schleife
 *     ({@link Engine#drainRetraction}, {@link Engine#linkRemoved}) zieht
 *     ihn zurueck, bevor ein Schritt laeuft. Solange ein Kandidat im Todo
 *     steht, ist der Match unter seinem Schluessel also sein eigener. Ein
 *     Host, der das oeffentliche retractMatch selbst fuer einen solchen
 *     Match ruft, verlaesst diese Grundlage, unter Pruefung wirft dann
 *     der Schritt.
 * <li>Ein Schluessel wird als angewandt vermerkt, wenn sein Kandidat
 *     gezogen wird, und vergessen, wenn {@link Engine#retractMatch} ihn
 *     aus {@code byKey} nimmt. Der Vermerk besteht also genau so lange,
 *     wie der Match, fuer den er gemacht wurde, unter dem Schluessel
 *     steht, und kann als Marke an diesem Match gefuehrt werden.
 * </ul>
 *
 * <p>Mit Pruefung ({@link Engine#CHECKS} oder vom Test gewaehlt) fuehrt
 * {@link #check} die Menge der angewandten Schluessel wie 2.0.2 daneben.
 * Jede Frage und jede Aenderung vergleicht, eine Abweichung wirft.
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
