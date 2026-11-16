package net.sandrakessler.seesaw.engine;

import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.List;
import java.util.Random;

/**
 * Hauptprogramm des Differenztests gegen Java 2.0.2 ({@code jgate.sh diff}).
 * Dieselbe Quelle läuft gegen den Baum dieses Zweigs und gegen den von
 * {@code v2.0.2} (dort mit der {@code Worlds.java} ohne Index-Argument).
 * Deshalb nutzt sie nur, was beide Engines haben.
 *
 * <p>Aufruf: {@code DiffDump <welt> <richtung> <n | von-bis> [voll <seed>]}. Welten:
 * {@code followup}, {@code family}, {@code predicates},
 * {@code predicates_no_equals}. Richtungen: {@code gerichtet}
 * (Wellen mit Richtung, {@code routed}) und {@code ungerichtet}.
 * Die Seeds 1 bis n (oder von bis bis) und das Szenario je Seed sind die von
 * {@code AnchorIndexEquivalenceTest}.
 *
 * <p>Ausgabe je Szenario eine Zeile
 * {@code szenario seed=… wellen=… angewandt=… sha=…} (SHA-256 über alle
 * Abzüge des Szenarios), am Ende {@code summe szenarien=… wellen=…
 * angewandt=…}. Mit {@code voll} nur das Szenario dieses Seeds, statt der
 * Zeile die Abzüge selbst, je Welle nach einer Zeile {@code @@ seed=… welle=…}.
 *
 * <p>Gegenprobe: mit {@code -Dseesaw.diff.mutate=<seed>} wird im ersten
 * Szenario ab diesem Seed, das Wellen hat, eine Zeile des ersten Abzugs
 * verändert. Der Seed steht auf stderr ({@code mutiert seed=…}).
 */
public final class DiffDump {
    private DiffDump() {}

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("Aufruf: DiffDump <welt> <richtung> <n | von-bis> [voll <seed>]");
            System.exit(2);
        }
        Worlds.World world;
        int maxWaves = 14;
        switch (args[0]) {
            case "followup" -> world = Worlds.followup();
            case "family" -> { world = Worlds.family(); maxWaves = 24; }
            case "predicates" -> world = Worlds.predicates(true);
            case "predicates_no_equals" -> world = Worlds.predicates(false);
            default -> { System.err.println("unbekannte Welt " + args[0]); System.exit(2); return; }
        }
        boolean routed;
        switch (args[1]) {
            case "gerichtet" -> routed = true;
            case "ungerichtet" -> routed = false;
            default -> { System.err.println("unbekannte Richtung " + args[1]); System.exit(2); return; }
        }
        // "n" heißt die Seeds 1 bis n, "von-bis" die Seeds von bis bis.
        long from = 1, to;
        if (args[2].contains("-")) {
            from = Long.parseLong(args[2].substring(0, args[2].indexOf('-')));
            to = Long.parseLong(args[2].substring(args[2].indexOf('-') + 1));
        } else {
            to = Long.parseLong(args[2]);
        }
        long n = to - from + 1;
        boolean full = args.length > 4 && args[3].equals("voll");
        long only = full ? Long.parseLong(args[4]) : -1;
        long mutate = Long.getLong("seesaw.diff.mutate", -1);
        boolean mutated = false;
        // Der Weg, den die Engine dieses Zweigs von sich aus nimmt. Die Engine von 2.0.2
        // kennt nur einen Weg, dort ignoriert Worlds.run das Argument.
        boolean useIndex = !"false".equalsIgnoreCase(System.getProperty("seesaw.anchorIndex"));
        long waves = 0, applied = 0;
        StringBuilder out = new StringBuilder();
        for (long seed = full ? only : from; seed <= (full ? only : to); seed++) {
            Random rnd = new Random(seed * 0x9E3779B97F4A7C15L + world.name().hashCode());
            List<Worlds.Wave> scenario = Worlds.scenario(rnd, maxWaves);
            List<String> dumps = Worlds.run(world, routed, scenario, useIndex, false);
            if (mutate >= 0 && !mutated && seed >= mutate && !dumps.isEmpty()) {
                dumps.set(0, "#" + dumps.get(0));
                mutated = true;
                System.err.println("mutiert seed=" + seed);
            }
            int a = 0;
            MessageDigest sha = MessageDigest.getInstance("SHA-256");
            for (int w = 0; w < dumps.size(); w++) {
                String d = dumps.get(w);
                sha.update(d.getBytes(StandardCharsets.UTF_8));
                int at = d.indexOf("steps=");
                int end = d.indexOf('\n', at);
                for (int i = at + 6; i < end; i++) if (d.charAt(i) == 'a') a++;
                if (full) out.append("@@ seed=").append(seed).append(" welle=").append(w).append('\n').append(d);
            }
            if (!full) {
                StringBuilder hex = new StringBuilder();
                for (byte b : sha.digest()) hex.append(String.format("%02x", b));
                out.append("szenario seed=").append(seed).append(" wellen=").append(dumps.size())
                        .append(" angewandt=").append(a).append(" sha=").append(hex).append('\n');
            }
            waves += dumps.size();
            applied += a;
        }
        out.append("summe szenarien=").append(full ? 1 : n).append(" wellen=").append(waves)
                .append(" angewandt=").append(applied).append('\n');
        System.out.print(out);
    }
}
