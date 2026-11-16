package net.sandrakessler.seesaw.engine;

import java.util.Arrays;

import net.sandrakessler.seesaw.ident.Id;

/**
 * Schluessel eines Matches: Regelindex plus Ref-Folge, Gegenstueck zu
 * {@code (usize, Refs)} in {@code by_key} und {@code applied} auf der
 * Rust-Seite.
 *
 * <p>Gleich sind zwei Schluessel mit gleichem Regelindex und Id fuer Id
 * gleicher Folge, also genau dann, wenn {@link Engine#key} fuer beide
 * dieselbe Zeichenkette liefert. Der Hash wird einmal im Konstruktor
 * berechnet. Bis hierher fuehrte die Engine {@code byKey} mit dieser
 * Zeichenkette, gebaut bei jedem Eintragen, Nachschlagen und Entfernen.
 *
 * <p>Die Folge wird ohne Kopie gehalten und darf nicht veraendert
 * werden: der Hash folgte nicht.
 */
final class MatchKey {
    final int ruleIx;
    final Id[] refs;
    private final int hash;

    MatchKey(int rule, Id[] refs) {
        this.ruleIx = rule;
        this.refs = refs;
        this.hash = 31 * rule + Arrays.hashCode(refs);
    }

    @Override public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof MatchKey)) return false;
        MatchKey k = (MatchKey) o;
        return hash == k.hash && ruleIx == k.ruleIx && Arrays.equals(refs, k.refs);
    }

    @Override public int hashCode() { return hash; }
}
