package net.sandrakessler.seesaw.plan;

import net.sandrakessler.seesaw.rules.Predicate;

/**
 * Ein Knoten des Match-Patterns: Typ plus optionale Wertbedingung.
 *
 * <p>Die Bedingung ist ein {@link Predicate}, nicht eine Sammlung
 * aufgeloester Felder. Damit gelten die vier Normierungen aus Spec §6
 * hier automatisch: Vollmatch, keine Anker, enge Syntax-Teilmenge,
 * eigene Zahlengrammatik. Die fruehere Fassung prüfte Regex mit
 * {@code find()} (Teiltreffer) und Zahlen mit
 * {@code Double.parseDouble} und wich damit an zwei Punkten von der
 * Rust-Seite ab.
 */
public final class PatNode {
    public final int typ;
    /** Wertbedingung, null = keine. */
    public final Predicate predicate;

    public PatNode(int typ, Predicate predicate) {
        this.typ = typ;
        this.predicate = predicate;
    }

    /** Trifft die Bedingung zu? Ohne Bedingung immer. */
    public boolean predMatches(String v) {
        return predicate == null || predicate.matches(v);
    }
}
