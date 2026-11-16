package net.sandrakessler.seesaw.engine;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.List;

import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.Ident;
import net.sandrakessler.seesaw.plan.PatLink;
import net.sandrakessler.seesaw.plan.Rule;

/**
 * Die Matches, die auf einer Verbindung ruhen (Rust {@code links.rs}). Sie
 * stehen nicht unter ihr. {@link Engine#byElement} haelt nur die Knoten eines
 * Matches, und die Matches, die auf einer Verbindung ruhen, werden aus den
 * Listen ihrer beiden Enden gefunden, wenn eine entfernte Verbindung nach
 * ihnen fragt.
 *
 * <p>2.0.2 legte einen Match fuer jeden Link seines Musters (ausser einem
 * Wert-Join) unter der Id der Verbindung a→b ab, deren Positionen an a und b
 * gebunden sind, ob der Graph diese Verbindung hielt oder nicht. Die Id einer
 * Verbindung haengt nur von ihren beiden Enden ab
 * ({@link Ident#identConnection}), und ein Graph entfernt keine Verbindung.
 * Dieselben Matches finden sich also aus den Enden: die, die beide binden,
 * mit einem Link von der Position des einen zur Position des anderen. Sie
 * kommen in der Folge der Ablage (aufsteigend, ein Match einmal je solchem
 * Link).
 *
 * <p>Bleiben die Ids, die der Graph zur Hand nicht kennt. Ein Match kann auf
 * so einer ruhen: sein Muster liest einen richtungsfreien Link gegen die
 * Richtung, die der Graph haelt, oder der Host hat den Graphen ersetzt, seit
 * der Match eingetragen wurde (eine materialisierte Kopie laesst die
 * Tombstones weg). Die Enden lassen sich an so einer Id nicht ablesen. Diese
 * Links werden doch abgelegt, in einem eigenen Index ({@link Unheld}), den es
 * erst gibt, wenn ein Aufruf nach einer Id gefragt hat, die der Graph nicht
 * kennt.
 */
final class Links {
    /** Die Engine, deren Matches das sind. */
    private final Engine e;

    /**
     * Ids, mit denen {@link Engine#elementRemoved} gerufen wurde und unter
     * denen es keine Liste fand. So eine Id kann eine Verbindung sein, und
     * deren Enden kennt die Engine aus dem Graphen, den diese Tuer nicht
     * bekommt. Die Matches, die auf ihr ruhen, toetet der naechste Aufruf,
     * der den Graphen bringt ({@link #settleRemoved}), bevor er etwas anderes
     * tut. Dazwischen zieht kein Schritt einen Match, es sterben also
     * dieselben Matches wie beim Aufruf selbst. Eingetragen werden kann
     * dazwischen nur ueber {@link Engine#record(int, Id[])}, die Tuer ohne
     * Graph. Deshalb steht bei jeder Id die Zahl der Matches beim Aufruf
     * ({@link Removed#matches}), und spaeter eingetragene Matches sterben
     * nicht.
     */
    private final ArrayList<Removed> removed = new ArrayList<>();

    /** Der Index der Links, deren Verbindung der Graph nicht haelt. null,
     *  bis ein Aufruf nach einer Id fragt, die der Graph nicht kennt. */
    Unheld unheld;

    /** Pruefung unter {@link Engine#CHECKS}: die Links, abgelegt wie 2.0.2
     *  sie ablegte. null ohne Pruefung. */
    final HashMap<Id, IntList> filed = Engine.CHECKS ? new HashMap<>() : null;

    /** Eine vorgemerkte Id und die Zahl der Matches, als sie vorgemerkt
     *  wurde. */
    private record Removed(Id id, int matches) {}

    /**
     * Die Matches mit einem Link, dessen Verbindung ein Graph nicht haelt,
     * unter der Id, die diese Verbindung haette: aufsteigend, ein Match
     * einmal je solchem Link.
     *
     * <p>Gebaut aus dem ganzen Match-Speicher bei der ersten Frage nach einer
     * Id, die der Graph nicht kennt, von da an von {@link Engine#record}
     * weitergefuehrt. Er gilt einem Graphen: eine Verbindung, die dieser
     * Graph nicht haelt, wenn ein Match hier abgelegt wird, kann spaeter
     * entstehen (dann ist die Id bekannt, die Matches kommen aus ihren Enden,
     * und dieser Eintrag wird nicht mehr gelesen), eine, die er haelt, bleibt.
     * Fuer einen anderen Graphen wird der Index neu gebaut.
     */
    static final class Unheld {
        /** Der Graph, dem der Index gilt: seine Nummer ({@link Graph#uid}),
         *  die jeder Graph und jede Kopie fuer sich hat. Nicht die seiner
         *  Typtabelle: die ist ein oeffentliches Feld, in Rust setzt ein
         *  Host sie per Move in einen anderen Graphen (b9b3ce9). */
        final long graph;
        final HashMap<Id, IntList> byId = new HashMap<>();

        Unheld(long graph) {
            this.graph = graph;
        }
    }

    Links(Engine e) {
        this.e = e;
    }

    /**
     * Die Links des Matches {@code ix} stehen nicht unter ihren Verbindungen
     * (Rust {@code file_links}). Gibt es den Index der nicht gehaltenen
     * Links, kommen die Links dorthin, deren Verbindung der Graph nicht
     * haelt. Unter Pruefung kommen alle in {@link #filed}, mit der Id, die
     * 2.0.2 vergab: aus der Participation-Liste der Quelle, wo der Graph die
     * Verbindung haelt, sonst abgeleitet, beides dieselbe Id.
     */
    void fileLinks(Graph g, Rule rule, Id[] refs, int ix) {
        if (filed != null) {
            for (PatLink l : rule.patLinks) {
                if (l.kind == PatLink.Kind.SAME_VALUE) continue;
                if (l.from < refs.length && l.to < refs.length) {
                    Id c = g == null ? null
                            : g.connectionIdTyped(refs[l.from], refs[l.to], rule.patNodes.get(l.to).typ);
                    if (c == null) c = Ident.identConnection(refs[l.from], refs[l.to]);
                    filed.computeIfAbsent(c, x -> new IntList()).add(ix);
                }
            }
        }
        if (unheld != null) fileUnheld(g, rule, refs, ix);
    }

    /**
     * Legt die Links des Matches {@code ix}, deren Verbindung {@code g} nicht
     * haelt, im Index der nicht gehaltenen Links ab (Rust
     * {@code file_unheld}). Gilt der Index einem anderen Graphen, faellt er
     * weg: die naechste Frage baut ihn fuer den Graphen, mit dem sie kommt.
     * Ebenso ohne Graph ({@link Engine#record(int, Id[])}): dann ist offen,
     * welche Verbindungen gehalten werden.
     */
    private void fileUnheld(Graph g, Rule rule, Id[] refs, int ix) {
        if (g == null || unheld.graph != g.uid()) {
            unheld = null;
            return;
        }
        fileUnheldLinks(unheld.byId, g, rule, refs, ix);
    }

    /** Legt die Links von {@code refs} unter {@code rule}, deren Verbindung
     *  {@code g} nicht haelt, als Match {@code ix} in {@code byId} ab. */
    private static void fileUnheldLinks(HashMap<Id, IntList> byId, Graph g, Rule rule, Id[] refs, int ix) {
        for (PatLink l : rule.patLinks) {
            if (l.kind == PatLink.Kind.SAME_VALUE) continue;
            if (l.from < refs.length && l.to < refs.length
                    && g.connectionIdTyped(refs[l.from], refs[l.to], rule.patNodes.get(l.to).typ) == null) {
                byId.computeIfAbsent(Ident.identConnection(refs[l.from], refs[l.to]), x -> new IntList())
                        .add(ix);
            }
        }
    }

    /**
     * Die Matches unter {@code id} (Rust {@code filed_under}): die, die den
     * Knoten {@code id} binden, oder die, die auf der Verbindung {@code id}
     * ruhen. null: keiner. Fuer einen Knoten seine Liste in
     * {@link Engine#byElement} selbst (der Aufrufer aendert sie nicht, sie
     * waechst nur am Ende), fuer eine Verbindung eine Liste, die der Aufrufer
     * nicht aendert.
     */
    IntList filedUnder(Graph g, Id id) {
        IntList binding = e.byElement.get(id);
        if (binding != null) {
            // Ein Knoten: keine Verbindung ist Schluessel von byElement.
            if (filed != null && filed.containsKey(id)) {
                throw new IllegalStateException("filedUnder: unter dem Knoten " + hex(id)
                        + " sind Links abgelegt " + filed.get(id));
            }
            return binding;
        }
        Graph.Slot s = g.map.get(id);
        IntList resting;
        if (s != null && s.connection != null) {
            resting = restingOn(s.connection.source, s.connection.target);
        } else if (s == null || s.node == null) {
            // Weder ein Knoten noch eine Verbindung des Graphen.
            resting = unheldUnder(g, id);
        } else {
            // Ein Knoten, den kein Match bindet.
            resting = null;
        }
        if (filed != null) {
            int[] want = array(filed.get(id));
            int[] got = array(resting);
            if (!Arrays.equals(want, got)) {
                throw new IllegalStateException("filedUnder: die gefundenen Matches " + Arrays.toString(got)
                        + " weichen von den abgelegten " + Arrays.toString(want) + " ab, Id " + hex(id));
            }
        }
        return resting;
    }

    /**
     * Die Matches mit einem Link, dessen Positionen an {@code source} und
     * {@code target} gebunden sind, in dieser Richtung (Rust
     * {@code resting_on}): aufsteigend, ein Match einmal je solchem Link.
     * null: keiner.
     */
    IntList restingOn(Id source, Id target) {
        // So ein Match bindet beide Enden.
        IntList atSource = e.byElement.get(source);
        IntList atTarget = e.byElement.get(target);
        if (atSource == null || atTarget == null) return null;
        IntList shorter = atSource.size() <= atTarget.size() ? atSource : atTarget;
        IntList resting = null;
        int last = -1;
        for (int i = 0, n = shorter.size(); i < n; i++) {
            int mix = shorter.get(i);
            // Ein Match, der einen Knoten an zwei Positionen bindet, steht
            // zweimal in dessen Liste, ein Eintrag nach dem anderen.
            if (mix == last) continue;
            last = mix;
            Engine.MatchRec m = e.matches.get(mix);
            if (m.ruleIx >= e.rules.size()) continue;
            Id[] refs = m.refs;
            List<PatLink> links = e.rules.get(m.ruleIx).patLinks;
            for (int j = 0, k = links.size(); j < k; j++) {
                PatLink l = links.get(j);
                if (l.kind != PatLink.Kind.SAME_VALUE && l.from < refs.length && l.to < refs.length
                        && refs[l.from].equals(source) && refs[l.to].equals(target)) {
                    if (resting == null) resting = new IntList();
                    resting.add(mix);
                }
            }
        }
        return resting;
    }

    /**
     * Die Matches mit einem Link, dessen Verbindung die Id {@code id} hat,
     * fuer eine Id, die der Graph nicht kennt (Rust {@code unheld_under}).
     * Gelesen aus dem Index der nicht gehaltenen Links, der hier gebaut
     * wird, wenn es keinen fuer diesen Graphen gibt: ein Gang ueber den
     * Match-Speicher, einmal je Graph.
     */
    private IntList unheldUnder(Graph g, Id id) {
        if (unheld == null || unheld.graph != g.uid()) unheld = unheldOf(g);
        return unheld.byId.get(id);
    }

    /** Jeder Link jedes Matches, dessen Verbindung {@code g} nicht haelt,
     *  unter der Id dieser Verbindung (Rust {@code unheld_of}). */
    private Unheld unheldOf(Graph g) {
        Unheld u = new Unheld(g.uid());
        for (int mix = 0, n = e.matches.size(); mix < n; mix++) {
            Engine.MatchRec m = e.matches.get(mix);
            if (m.ruleIx >= e.rules.size()) continue;
            fileUnheldLinks(u.byId, g, e.rules.get(m.ruleIx), m.refs, mix);
        }
        return u;
    }

    /** {@link Engine#elementRemoved} fand unter {@code id} keine Liste. Es
     *  kann eine Verbindung sein: vorgemerkt fuer {@link #settleRemoved}
     *  (Rust {@code removed_unfiled}). */
    void removedUnfiled(Id id) {
        removed.add(new Removed(id, e.matches.size()));
    }

    /**
     * Toetet die Matches, die auf den Verbindungen ruhen, mit denen
     * {@link Engine#elementRemoved} seit dem letzten Aufruf mit Graph gerufen
     * wurde (Rust {@code settle_removed}). Jede oeffentliche Tuer der Engine,
     * die den Graphen nimmt und den Match-Speicher liest oder schreibt, ruft
     * das zuerst.
     */
    void settleRemoved(Graph g) {
        if (removed.isEmpty()) return;
        for (Removed r : removed) {
            IntList resting = filedUnder(g, r.id());
            if (resting != null) e.kill(resting, r.matches());
        }
        removed.clear();
    }

    /** Pruefung unter {@link Engine#CHECKS}: unter {@code id} sind keine
     *  Links abgelegt. {@code where}: die Tuer, die fragt. */
    void checkNotFiled(String where, Id id) {
        if (filed != null && filed.containsKey(id)) {
            throw new IllegalStateException(where + ": unter " + hex(id) + " sind Links abgelegt "
                    + filed.get(id) + ", gefragt wird nach Knoten");
        }
    }

    private static int[] array(IntList l) {
        return l == null ? new int[0] : l.toArray();
    }

    private static String hex(Id id) {
        return HexFormat.of().formatHex(id.b);
    }
}
