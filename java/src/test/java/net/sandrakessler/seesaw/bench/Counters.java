package net.sandrakessler.seesaw.bench;

import java.lang.reflect.Field;
import java.lang.reflect.Modifier;

/**
 * Paket-private Zähler der Engine ({@code searches},
 * {@code seedsByIndex}), per Reflexion gelesen. Die Messstrecke wird aus
 * dem neuesten Stand gebaut und läuft gegen die Hauptklassen älterer
 * Stände ({@code java/bench/jsuite.sh}). Ein direkter Feldzugriff
 * scheitert dort mit {@code NoSuchFieldError}, wenn ein Stand das Feld
 * nicht hat oder anders führt; die Reflexion liefert dann -1.
 */
public final class Counters {
    private Counters() {}

    /** Wert des {@code long}-Felds {@code name} von {@code o}, -1 wo
     *  das Feld fehlt, statisch ist oder kein {@code long} ist. */
    public static long of(Object o, String name) {
        try {
            Field f = o.getClass().getDeclaredField(name);
            if (f.getType() != long.class || Modifier.isStatic(f.getModifiers())) return -1;
            f.setAccessible(true);
            return f.getLong(o);
        } catch (NoSuchFieldException | IllegalAccessException | RuntimeException ex) {
            return -1;
        }
    }
}
