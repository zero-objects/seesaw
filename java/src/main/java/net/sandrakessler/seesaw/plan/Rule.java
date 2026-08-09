package net.sandrakessler.seesaw.plan;

import java.util.ArrayList;
import java.util.List;


public final class Rule {
    public String name;
    public long rank;
    public List<PatNode> patNodes;
    public List<PatLink> patLinks;
    public List<CreateNode> createNodes;
    public List<int[]> createLinks;
    public List<String> inputTypes;
    /** (Corr-Typ, Anker-Pos, Endpunkt-Typ) — ALLE müssen präsent sein. */
    public List<Object[]> corrRecognition = new ArrayList<>();
}
