package net.sandrakessler.seesaw.engine;

import java.util.Arrays;

/**
 * Liste von Indizes in den Match-Speicher, je Element von
 * {@code Engine.byElement} (Rust {@code Ixs}). Die meisten Elemente
 * stehen in wenigen Matches: bis {@link #INLINE} Indizes liegen in
 * Feldern des Objekts, erst eine laengere Liste bekommt ein Array.
 * Reihenfolge des Einfuegens, ohne Boxing. Es wird nur angehaengt.
 */
final class IntList {
    /** Indizes, die die Liste ohne Array haelt (Rust {@code IXS_INLINE}). */
    static final int INLINE = 4;

    private int size;
    private int i0, i1, i2, i3;
    /** Ab dem fuenften Eintrag alle Eintraege, vorher null. */
    private int[] heap;

    void add(int value) {
        if (heap != null) {
            if (size == heap.length) heap = Arrays.copyOf(heap, 2 * size);
            heap[size++] = value;
            return;
        }
        switch (size) {
            case 0 -> i0 = value;
            case 1 -> i1 = value;
            case 2 -> i2 = value;
            case 3 -> i3 = value;
            default -> {
                heap = new int[2 * INLINE];
                heap[0] = i0;
                heap[1] = i1;
                heap[2] = i2;
                heap[3] = i3;
                heap[INLINE] = value;
            }
        }
        size++;
    }

    int size() { return size; }

    int get(int i) {
        if (i < 0 || i >= size) throw new IndexOutOfBoundsException("Index " + i + ", Laenge " + size);
        if (heap != null) return heap[i];
        return switch (i) {
            case 0 -> i0;
            case 1 -> i1;
            case 2 -> i2;
            default -> i3;
        };
    }

    /** Die Eintraege als Kopie. */
    int[] toArray() {
        int[] out = new int[size];
        for (int i = 0; i < size; i++) out[i] = get(i);
        return out;
    }

    /** Wie {@code ArrayList.toString}: {@code [1, 2]}. */
    @Override public String toString() {
        StringBuilder sb = new StringBuilder().append('[');
        for (int i = 0; i < size; i++) {
            if (i > 0) sb.append(", ");
            sb.append(get(i));
        }
        return sb.append(']').toString();
    }
}
