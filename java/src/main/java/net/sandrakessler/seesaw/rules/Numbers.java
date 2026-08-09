package net.sandrakessler.seesaw.rules;

/**
 * Eigene Zahlengrammatik, Spiegel von
 * {@code rules/predicate.rs::parse_number}.
 *
 * <p>Grammatik: {@code [+-]? Ziffern [. Ziffern] [eE [+-]? Ziffern]}.
 * Kein Hex-Float, keine {@code d}/{@code f}-Suffixe, kein {@code inf},
 * kein {@code NaN}.
 *
 * <p>Der Grund für eine eigene Grammatik statt
 * {@link Double#parseDouble}: Javas Parser nimmt {@code "1d"},
 * {@code "0x1p3"}, {@code "Infinity"} und führende Leerzeichen an,
 * Rusts {@code str::parse::<f64>} nimmt {@code "inf"} und {@code "NaN"}
 * an. Beide Mengen sind verschieden, und beide sind weiter als das,
 * was ein Zahlenbereich in einer Regeldatei bedeuten soll.
 */
final class Numbers {
    private Numbers() {}

    /** Zahl nach obiger Grammatik, oder null. */
    public static Double parse(String s) {
        if (s == null) return null;
        int i = 0;
        int n = s.length();
        if (i < n && (s.charAt(i) == '+' || s.charAt(i) == '-')) i++;
        int d0 = i;
        while (i < n && isDigit(s.charAt(i))) i++;
        if (i == d0) return null;
        if (i < n && s.charAt(i) == '.') {
            i++;
            int d1 = i;
            while (i < n && isDigit(s.charAt(i))) i++;
            if (i == d1) return null;
        }
        if (i < n && (s.charAt(i) == 'e' || s.charAt(i) == 'E')) {
            i++;
            if (i < n && (s.charAt(i) == '+' || s.charAt(i) == '-')) i++;
            int d2 = i;
            while (i < n && isDigit(s.charAt(i))) i++;
            if (i == d2) return null;
        }
        if (i != n) return null;
        try {
            return Double.valueOf(s);
        } catch (NumberFormatException e) {
            return null;
        }
    }

    /**
     * ASCII-Ziffer. Nicht {@link Character#isDigit}, das jede
     * Unicode-Dezimalziffer annimmt (arabisch-indisch, Devanagari und
     * so fort) — Rusts {@code is_ascii_digit} tut das nicht.
     */
    private static boolean isDigit(char c) {
        return c >= '0' && c <= '9';
    }
}
