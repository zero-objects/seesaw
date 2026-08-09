package net.sandrakessler.seesaw.rules;

import java.util.regex.Pattern;

/**
 * Prüfung der zulässigen Regex-Teilmenge, Spiegel von
 * {@code rules/predicate.rs::find_forbidden_syntax}.
 *
 * <p>Verboten sind Anker ({@code ^}/{@code $}) außerhalb einer
 * Zeichenklasse, Lookaround ({@code (?=}, {@code (?!}, {@code (?<=},
 * {@code (?<!}), benannte Gruppen in beiden Schreibweisen
 * ({@code (?<name>} und {@code (?P<name>}), {@code \b} {@code \B}
 * {@code \p{...}} {@code \P{...}}, Rückwärtsverweise und possessive
 * Quantoren in allen vier Formen ({@code *+}, {@code ++}, {@code ?+},
 * <code>{n,m}+</code>).
 *
 * <p>Die Prüfung ist kontextsensitiv, nicht eine Teilstring-Suche:
 * {@code ^} in {@code [^a]} ist kein Anker, {@code \*+} ist kein
 * possessiver Quantor, und {@code \\b} ist kein Wortgrenzen-Escape.
 * Ein einziger Durchlauf mit zwei Zuständen trägt das: innerhalb einer
 * Zeichenklasse, und ob der zuletzt gesehene Token ein gerade
 * geschlossener Quantor war.
 *
 * <p>Über Rust hinaus verboten: die Klassen-Kurzformen {@code \d}
 * {@code \D} {@code \w} {@code \W} {@code \s} {@code \S}. Sie bedeuten
 * in den beiden Sprachen nicht dasselbe — Rusts {@code regex} ist im
 * Unicode-Modus, {@code \d} deckt dort jede Unicode-Dezimalziffer ab,
 * Javas {@code \d} ohne {@code UNICODE_CHARACTER_CLASS} nur
 * {@code [0-9]}. Eine Angleichung über das Java-Flag käme nahe heran,
 * aber nicht beweisbar gleich ({@code \w} ist über {@code Alpha}
 * gegen {@code Alphabetic} definiert). Wer Ziffern meint, schreibt
 * {@code [0-9]}, und dann stimmen beide Sprachen überein. Siehe den
 * offenen Punkt im Implementierungsbericht.
 *
 * <p>Grenze, wie in Rust: verschachtelte Mengenoperationen in
 * Zeichenklassen ({@code [a-z&&[^aeiou]]}) werden nicht erkannt. Die
 * enge Syntax-Teilmenge sieht sie ohnehin nicht vor.
 */
final class RegexSubset {
    private RegexSubset() {}

    /** Verbotene Konstruktion, oder null wenn das Muster zulässig ist. */
    public static String findForbidden(String pattern) {
        int i = 0;
        int n = pattern.length();
        boolean inClass = false;
        // Der zuletzt gesehene Token außerhalb einer Klasse war ein
        // gerade geschlossener Quantor. Nur ein direkt folgendes
        // zweites '+' macht daraus einen possessiven Quantor.
        boolean prevWasQuantifierClose = false;

        while (i < n) {
            char c = pattern.charAt(i);

            if (c == '\\') {
                if (i + 1 >= n) {
                    // Abgeschnittenes Escape: ungültige Syntax, den
                    // Fehler macht Pattern.compile (BAD_REGEX).
                    return null;
                }
                char next = pattern.charAt(i + 1);
                switch (next) {
                    case 'b': case 'B': case 'p': case 'P':
                        return "\\" + next;
                    case 'd': case 'D': case 'w': case 'W': case 's': case 'S':
                        return "\\" + next + " (class shorthand, use an explicit "
                                + "character class such as [0-9])";
                    default:
                        break;
                }
                if (next >= '0' && next <= '9') return "backreference";
                // Escapte Literale (\^ \$ \* \+ \? \\ \[ \] \{ \} \.):
                // entschärft, kein Anker- oder Quantorzeichen. Ein
                // direkt folgender einfacher Quantor ist normale
                // Quantor-Nutzung, kein possessives Doppel.
                i += 2;
                prevWasQuantifierClose = false;
                continue;
            }

            // Grenzen der Zeichenklasse.
            if (!inClass && c == '[') {
                inClass = true;
                i++;
                if (i < n && pattern.charAt(i) == '^') {
                    i++; // Klassen-Negation, kein Anker.
                }
                if (i < n && pattern.charAt(i) == ']') {
                    i++; // ']' an erster Stelle ist ein Literal (POSIX).
                }
                prevWasQuantifierClose = false;
                continue;
            }
            if (inClass && c == ']') {
                inClass = false;
                i++;
                prevWasQuantifierClose = false;
                continue;
            }
            if (inClass) {
                // Innerhalb der Klasse sind Metazeichen Literale.
                i++;
                prevWasQuantifierClose = false;
                continue;
            }

            // Gruppenmarken "(?...": vor der allgemeinen
            // Quantor-Erkennung, damit das '?' darin nicht selbst als
            // Quantorzeichen gelesen wird.
            if (c == '(' && i + 1 < n && pattern.charAt(i + 1) == '?') {
                if (pattern.startsWith("(?=", i)) return "(?=";
                if (pattern.startsWith("(?!", i)) return "(?!";
                if (pattern.startsWith("(?P<", i)) return "(?P<";
                // deckt (?<name>...) und Lookbehind (?<=...)/(?<!...) ab.
                if (pattern.startsWith("(?<", i)) return "(?<";
                // Erlaubte Marken, etwa "(?:" oder "(?i)": "(?" verbrauchen.
                i += 2;
                prevWasQuantifierClose = false;
                continue;
            }

            switch (c) {
                case '^':
                case '$':
                    return String.valueOf(c);
                case '*':
                case '+':
                case '?':
                    if (prevWasQuantifierClose && c == '+') {
                        return "possessive quantifier";
                    }
                    prevWasQuantifierClose = true;
                    break;
                case '{': {
                    // {n,m}-Intervall: bis zur schließenden '}' springen.
                    int rel = pattern.indexOf('}', i);
                    if (rel >= 0) {
                        i = rel + 1;
                        prevWasQuantifierClose = true;
                        continue;
                    }
                    // Keine schließende '}': ungültige Syntax, den
                    // Fehler macht Pattern.compile.
                    prevWasQuantifierClose = false;
                    break;
                }
                default:
                    prevWasQuantifierClose = false;
                    break;
            }
            i++;
        }
        return null;
    }
}
