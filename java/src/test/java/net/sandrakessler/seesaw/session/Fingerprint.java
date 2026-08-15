package net.sandrakessler.seesaw.session;

import net.sandrakessler.seesaw.graph.Conn;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.graph.Part;
import net.sandrakessler.seesaw.ident.Id;

import java.nio.charset.StandardCharsets;
import java.util.TreeSet;

/**
 * Endzustands-Fingerprint eines Graphenen: alive-Knotenzahl und
 * FNV-1a-64 über eine kanonische Textform. Spiegel von
 * {@code fingerprint_v2} in
 * {@code crates/seesaw-bench-pil2spell/tests/hermann_pipeline.rs} —
 * beide Seiten müssen dieselbe Bytefolge hashen, sonst vergleicht der
 * Äquivalenz-Test nichts.
 *
 * <p>Form: alive-Knoten aufsteigend nach Id-Hex, je Knoten
 * {@code <hex>{<ziel1>;<ziel2>}|}, Ziele die alive-Enden lebendiger
 * ausgehender Verbindungen, aufsteigend und dedupliziert.
 *
 * <p>Anders als die Anwendungs-Zahl je Phase hängt der Fingerprint an
 * den Ids — er ist das Gate, das eine abweichende Identitäts-Ableitung
 * sichtbar macht.
 */
final class Fingerprint {

    private Fingerprint() {}

    static final class Result {
        final int aliveNodes;
        final String hex;

        Result(int aliveNodes, String hex) { this.aliveNodes = aliveNodes; this.hex = hex; }
    }

    static String hex(Id id) {
        StringBuilder sb = new StringBuilder(64);
        for (byte b : id.b) sb.append(String.format("%02x", b & 0xFF));
        return sb.toString();
    }

    static Result of(Graph g) {
        TreeSet<Id> alive = new TreeSet<>();
        for (Graph.Slot s : g.map.values()) {
            if (s.node != null && s.node.status.alive()) alive.add(s.node.id);
        }
        StringBuilder sb = new StringBuilder();
        for (Id id : alive) {
            sb.append(hex(id));
            TreeSet<String> outs = new TreeSet<>();
            for (Part p : g.parts(id)) {
                if (!p.outgoing) continue;
                Conn c = g.conn(p.connection);
                if (c == null || !c.status.alive()) continue;
                if (!alive.contains(p.other)) continue;
                outs.add(hex(p.other));
            }
            sb.append('{').append(String.join(";", outs)).append('}').append('|');
        }
        long h = 0xcbf29ce484222325L;
        for (byte b : sb.toString().getBytes(StandardCharsets.UTF_8)) {
            h ^= (b & 0xFF);
            h *= 0x100000001b3L;
        }
        return new Result(alive.size(), String.format("%016x", h));
    }
}
