package net.sandrakessler.seesaw.engine;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assertions.fail;

import java.util.List;
import java.util.Random;

import org.junit.jupiter.api.Test;

/**
 * Der Weg mit {@link AnchorIndex} gegen den Weg ohne, auf zufälligen
 * Modellen und Wellen.
 *
 * <p>Jedes Szenario läuft durch zwei Engines über demselben Regelsatz,
 * eine mit und eine ohne Index, auf je einem eigenen Graphen. Nach
 * jeder Welle wird der Abzug verglichen ({@link Worlds#dump}): Graph,
 * Kaskade, Rückgaben von {@code step}, dazu die eingetragenen Matches
 * in ihrer Reihenfolge, das To-do und die angewandten Matches.
 *
 * <p>Die Zahl der Fälle je Eigenschaft lässt sich mit der
 * Systemeigenschaft {@code seesaw.cases} vervielfachen (Faktor).
 */
final class AnchorIndexEquivalenceTest {

    private static int cases(int standard) {
        return standard * Integer.getInteger("seesaw.cases", 1);
    }

    private static void same(String what, long seed, List<String> plain, List<String> indexed) {
        assertEquals(plain.size(), indexed.size(), what + " seed " + seed + ": Zahl der Wellen");
        for (int w = 0; w < plain.size(); w++) {
            if (!plain.get(w).equals(indexed.get(w))) {
                fail(what + " seed " + seed + ": Welle " + w + " weicht ab\n--- ohne Index\n"
                        + plain.get(w) + "\n--- mit Index\n" + indexed.get(w));
            }
        }
    }

    /** Zählt, damit ein Lauf ohne ein einziges angewandtes Match
     *  auffällt: dann prüfte die Eigenschaft nichts. */
    private static int applied(List<String> dumps) {
        int n = 0;
        for (String d : dumps) {
            int at = d.indexOf("steps=");
            int end = d.indexOf('\n', at);
            for (int i = at + 6; i < end; i++) if (d.charAt(i) == 'a') n++;
        }
        return n;
    }

    private static void world(Worlds.World world, int maxWaves, int cases) {
        int applied = 0;
        for (long seed = 1; seed <= cases; seed++) {
            Random rnd = new Random(seed * 0x9E3779B97F4A7C15L + world.name().hashCode());
            boolean routed = rnd.nextBoolean();
            List<Worlds.Wave> waves = Worlds.scenario(rnd, maxWaves);
            // In der Haelfte der Faelle geht auch der Seed ueber den
            // Index, sonst waehlt ihn die Schwelle bei so kleinen
            // Modellen nie.
            List<String> plain = Worlds.run(world, routed, waves, false, false);
            List<String> indexed = Worlds.run(world, routed, waves, true, seed % 2 == 0);
            same(world.name(), seed, plain, indexed);
            applied += applied(plain);
        }
        assertTrue(applied > cases / 4, world.name() + ": nur " + applied + " angewandte Schritte");
    }

    @Test
    void followupMitUndOhneIndexGleich() {
        world(Worlds.followup(), 14, cases(600));
    }

    /** Je Art von Prädikat eine Regel. Das Blatt mit dem Prädikat kommt
     *  allein als neuer Knoten oder mit geändertem Wert: es steht dann
     *  selbst an der verankerten Position. */
    @Test
    void praedikateMitUndOhneIndexGleich() {
        world(Worlds.predicates(true), 14, cases(600));
    }

    /** Ohne die Regel mit {@code equals}: der Wert des verankerten
     *  Knotens wird dann nur der anderen Arten wegen aufgelöst. */
    @Test
    void praedikateOhneEqualsMitUndOhneIndexGleich() {
        world(Worlds.predicates(false), 14, cases(600));
    }

    /** Ein Fall dieser Welt kostet rund eine halbe Sekunde (gemessen:
     *  300 Fälle in 155 s auf einem belasteten Rechner), deshalb im
     *  Standard wenige. */
    @Test
    void familyMitUndOhneIndexGleich() {
        world(Worlds.family(), 24, cases(24));
    }

    private static void lang(String what, boolean lateContext, int nodes, int ops, boolean hub,
            boolean addsOnly, int cases) {
        int applied = 0;
        for (long seed = 1; seed <= cases; seed++) {
            Random rnd = new Random(seed * 0x9E3779B97F4A7C15L + what.hashCode());
            LangWorld.Scenario s = LangWorld.scenario(rnd, lateContext, nodes, ops, hub, addsOnly);
            List<String> plain = LangWorld.run(s, false, false);
            List<String> indexed = LangWorld.run(s, true, seed % 2 == 0);
            same(what, seed, plain, indexed);
            applied += applied(plain);
        }
        assertTrue(applied > cases, what + ": nur " + applied + " angewandte Schritte");
    }

    /** Nur Hinzufügen: die Anfangswelle und einzelne neue Knoten. */
    @Test
    void sprachpackHinzufuegenGleich() {
        lang("adds", false, 24, 8, false, true, cases(400));
    }

    /** Hinzufügen, Ändern, Löschen und Seeds in beliebiger Folge. */
    @Test
    void sprachpackWellenGleich() {
        lang("waves", false, 16, 12, false, false, cases(400));
    }

    /** Mit der Regel, deren Match erst durch die Korrespondenz eines
     *  Geschwisters entsteht. */
    @Test
    void sprachpackWellenMitSpaetemKontextGleich() {
        lang("late", true, 16, 12, false, false, cases(300));
    }

    /** Die meisten Knoten hängen an einem: ein Knoten mit vielen
     *  Nachbarn. Die Regel {@code Adopt} hat dort einen Match je Paar
     *  von Geschwistern, die Zahl der Matches wächst mit dem Quadrat
     *  der Nachbarn. */
    @Test
    void sprachpackMitVielenNachbarnGleich() {
        lang("hub", true, 60, 12, true, false, cases(20));
    }
}
