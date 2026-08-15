package net.sandrakessler.seesaw.plan;

import java.util.ArrayList;
import java.util.List;


public final class Rule {
    public enum Direction {
        FORWARD("forward"), BACKWARD("backward"), UNDIRECTED("undirected");

        public final String wire;

        Direction(String wire) { this.wire = wire; }

        public static Direction fromWire(String wire) {
            for (Direction d : values()) if (d.wire.equals(wire)) return d;
            throw new IllegalArgumentException("unknown rule direction: " + wire);
        }
    }

    public String name;
    public long rank;
    public Direction direction = Direction.UNDIRECTED;
    public List<PatNode> patNodes;
    public List<PatLink> patLinks;
    public List<CreateNode> createNodes;
    public List<int[]> createLinks;
    public List<String> inputTypes;
    /** (Corr-Typ, Anker-Pos, Endpunkt-Typ) — ALLE müssen präsent sein. */
    public List<Object[]> corrRecognition = new ArrayList<>();
}
