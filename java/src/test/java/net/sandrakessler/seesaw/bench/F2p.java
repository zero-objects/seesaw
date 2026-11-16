package net.sandrakessler.seesaw.bench;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.engine.Langpack;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;
import net.sandrakessler.seesaw.plan.Rule;
import net.sandrakessler.seesaw.rules.Rules;

/**
 * Workload „Families to Persons“ (Benchmarx) der Rust-Suite
 * ({@code benches/suite/f2p.rs}): fünf Regeln, jede mit eigenen
 * Knotentypen, über ein breites und flaches Modell. Vier Mitglieder je
 * Familie, ein Modell aus {@code n} Familien braucht {@code 4 n + 1}
 * angewandte Schritte.
 *
 * <p>Regelsatz, Modell und Ablauf jeder Zelle sind die der Rust-Seite,
 * ohne die Kapazitätsangaben ({@code hints}), die Java nicht hat. Das
 * Rust-Modell kommt ohne Zufallsgenerator aus: {@code delta-delete}
 * löscht Mitglieder in festem Abstand. Fingerprint und Ergebnis wie
 * {@link Langpack}.
 */
public final class F2p {
    private F2p() {}

    private static String memberRule(String name, String role, int rank) {
        String person = role.equals("Father") || role.equals("Sons") ? "Male" : "Female";
        return "{\"name\":\"" + name + "\",\"rank\":" + rank + ","
                + "\"left\":{\"anchor\":\"fam\",\"nodes\":["
                + "{\"name\":\"fam\",\"type\":\"Family\"},"
                + "{\"name\":\"role\",\"type\":\"" + role + "\"},"
                + "{\"name\":\"member\",\"type\":\"Member\"},"
                + "{\"name\":\"first\",\"type\":\"firstName\"}],"
                + "\"links\":[[\"fam\",\"role\"],[\"role\",\"member\"],[\"member\",\"first\"]]},"
                + "\"right\":{\"anchor\":\"person\",\"nodes\":["
                + "{\"name\":\"person\",\"type\":\"" + person + "\"},"
                + "{\"name\":\"pname\",\"type\":\"name\"}],"
                + "\"links\":[[\"person\",\"pname\"]]},"
                + "\"corrs\":[{\"type\":\"PersonCorr\",\"left\":\"member\",\"right\":\"person\","
                + "\"role\":\"establishes\",\"bindings\":[{\"left\":\"first\",\"right\":\"pname\"}]}]}";
    }

    private static String registerRule() {
        return "{\"name\":\"Register\",\"rank\":1000,"
                + "\"left\":{\"anchor\":\"l0\",\"nodes\":["
                + "{\"name\":\"l0\",\"type\":\"FamilyRegister\"},"
                + "{\"name\":\"l1\",\"type\":\"regName\"}],"
                + "\"links\":[[\"l0\",\"l1\"]]},"
                + "\"right\":{\"anchor\":\"r0\",\"nodes\":["
                + "{\"name\":\"r0\",\"type\":\"PersonRegister\"},"
                + "{\"name\":\"r1\",\"type\":\"name\"}],"
                + "\"links\":[[\"r0\",\"r1\"]]},"
                + "\"corrs\":[{\"type\":\"RegisterCorr\",\"left\":\"l0\",\"right\":\"r0\","
                + "\"role\":\"establishes\",\"bindings\":[{\"left\":\"l1\",\"right\":\"r1\"}]}]}";
    }

    public static String rulesJson() {
        return "{\"format\":3,\"name\":\"f2p\",\"rules\":["
                + registerRule() + ","
                + memberRule("Father_2_Male", "Father", 850) + ","
                + memberRule("Mother_2_Female", "Mother", 840) + ","
                + memberRule("Sons_2_Male", "Sons", 830) + ","
                + memberRule("Daughters_2_Female", "Daughters", 820) + "]}";
    }

    private static final String[] ROLES = {"Father", "Mother", "Sons", "Daughters"};

    /** Register, darunter die Familien mit ihren vier Rollen, je Rolle
     *  ein Mitglied mit Vornamen. Öffentlich für {@code MatcherTest}
     *  (Paket engine). */
    public static final class Model {
        public final Graph g = new Graph();
        public final Map<Id, String> vs = new HashMap<>();
        final Id reg;
        /** Jeder Baseline-Knoten, in der Reihenfolge des Anlegens. */
        public final List<Id> all = new ArrayList<>();
        /** Die {@code Member}-Knoten, in der Reihenfolge des Anlegens. */
        public final List<Id> members = new ArrayList<>();
        int families;

        public Model(int families) {
            reg = g.addBaseline("reg", "FamilyRegister");
            Id regname = g.addBaseline("reg/name", "regName");
            g.connect(reg, regname, St.SOLID);
            vs.put(regname, "R");
            all.add(reg);
            all.add(regname);
            for (int i = 0; i < families; i++) grow();
        }

        /** Fügt eine Familie mit ihren vier Mitgliedern hinzu; liefert die
         *  neuen Knoten. */
        public List<Id> grow() {
            int i = families++;
            Id f = g.addBaseline("f" + i, "Family");
            g.connect(reg, f, St.SOLID);
            List<Id> fresh = new ArrayList<>(13);
            fresh.add(f);
            for (String role : ROLES) {
                Id r = g.addBaseline("f" + i + "/" + role, role);
                g.connect(f, r, St.SOLID);
                Id m = g.addBaseline("f" + i + "/" + role + "/m", "Member");
                g.connect(r, m, St.SOLID);
                Id leaf = g.addBaseline("f" + i + "/" + role + "/m/fn", "firstName");
                g.connect(m, leaf, St.SOLID);
                vs.put(leaf, role + "-" + i);
                members.add(m);
                fresh.add(r);
                fresh.add(m);
                fresh.add(leaf);
            }
            all.addAll(fresh);
            return fresh;
        }
    }

    private static long saturate(Engine e, Graph g, Map<Id, String> vs) {
        long steps = 0;
        while (e.step(g, vs) != null) steps++;
        return steps;
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new IllegalStateException(what);
    }

    private static void applied(Engine e, int families) {
        check(e.cascade.size() == 4 * families + 1,
                "Kaskade " + e.cascade.size() + ", erwartet " + (4 * families + 1));
    }

    /** Batch durch {@code Engine.run} (Seed, dann Schritte bis zur
     *  Sättigung). */
    public static Langpack.Outcome batch(int families, Section measured) {
        Model m = new Model(families);
        List<Rule> rules = Rules.load(rulesJson(), m.g);
        Engine e = new Engine(rules);
        measured.start();
        Engine.Termination t = e.run(m.g, m.vs, 100_000_000);
        measured.stop();
        check(t == Engine.Termination.DUPLICATION, "Verdikt " + t);
        applied(e, families);
        return Langpack.outcome(m.g, e, m.vs, e.cascade.size(), measured);
    }

    /** Das ganze Modell als ein Add-Delta. */
    public static Langpack.Outcome add(int families, Section measured) {
        Model m = new Model(families);
        List<Rule> rules = Rules.load(rulesJson(), m.g);
        Engine e = new Engine(rules);
        measured.start();
        e.elementsAdded(m.g, m.vs, m.all);
        long steps = saturate(e, m.g, m.vs);
        measured.stop();
        applied(e, families);
        return Langpack.outcome(m.g, e, m.vs, steps, measured);
    }

    /** Ein synchronisiertes Modell mit Engine, Ausgang der Delta-Zellen. */
    private static Engine synchronised(Model m) {
        List<Rule> rules = Rules.load(rulesJson(), m.g);
        Engine e = new Engine(rules);
        e.elementsAdded(m.g, m.vs, m.all);
        saturate(e, m.g, m.vs);
        return e;
    }

    /** {@code delta} neue Familien auf einem synchronisierten Modell,
     *  eine Welle je Familie. */
    public static Langpack.Outcome deltaAdd(int families, int delta, Section measured) {
        Model m = new Model(families);
        Engine e = synchronised(m);
        measured.start();
        long steps = 0;
        for (int i = 0; i < delta; i++) {
            List<Id> fresh = m.grow();
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementsAdded(m.g, m.vs, fresh);
            steps += saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        measured.stop();
        applied(e, families + delta);
        return Langpack.outcome(m.g, e, m.vs, steps, measured);
    }

    /** {@code delta} Löschungen eines Mitglieds auf einem
     *  synchronisierten Modell, eine Welle je Löschung. Die Mitglieder
     *  liegen gleichmäßig über das Modell verteilt. */
    public static Langpack.Outcome deltaDelete(int families, int delta, Section measured) {
        Model m = new Model(families);
        Engine e = synchronised(m);
        int stride = Math.max(1, m.members.size() / delta);
        measured.start();
        long steps = 0;
        for (int i = 0; i < delta; i++) {
            Id member = m.members.get((i * stride) % m.members.size());
            m.g.setNodeStatus(member, St.TOMBSTONE);
            e.admitDelta(List.of(Engine.DeltaDomain.SOURCE));
            e.elementRemoved(member);
            e.elementDeleted(m.g, member);
            steps += saturate(e, m.g, m.vs);
            e.consolidate(m.g);
        }
        measured.stop();
        return Langpack.outcome(m.g, e, m.vs, steps, measured);
    }
}
