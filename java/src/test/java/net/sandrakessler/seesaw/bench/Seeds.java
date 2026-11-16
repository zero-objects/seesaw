package net.sandrakessler.seesaw.bench;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.Map;

import net.sandrakessler.seesaw.engine.Engine;
import net.sandrakessler.seesaw.graph.Graph;
import net.sandrakessler.seesaw.ident.Id;

/**
 * Der Seed auf mehreren Threads ({@code Engine.seedWithThreads}) fuer
 * die Zellen {@code .../reseed/.../t<n>}, per Reflexion gerufen. Die
 * Messstrecke laeuft gegen die Hauptklassen aelterer Staende
 * ({@code java/bench/jsuite.sh}), die die Methode nicht haben: dort ist
 * ein Thread {@link Engine#seed}, mehr Threads sind nicht verfuegbar
 * ({@link Unavailable}).
 */
public final class Seeds {
    private Seeds() {}

    /** Die Zelle verlangt {@code seedWithThreads}, der Stand hat es
     *  nicht. {@link JavaSuite} schreibt dann eine Zeile
     *  {@code nicht_verfuegbar} statt eines Ergebnisses. */
    public static final class Unavailable extends RuntimeException {
        Unavailable(String message) {
            super(message);
        }
    }

    /** {@code Engine.seedWithThreads(Graph, Map, int)}, null wo der Stand
     *  sie nicht hat. */
    private static final Method WITH_THREADS = lookup();

    private static Method lookup() {
        try {
            return Engine.class.getMethod("seedWithThreads", Graph.class, Map.class, int.class);
        } catch (NoSuchMethodException ex) {
            return null;
        }
    }

    /** Wirft {@link Unavailable}, wenn der Stand {@code threads} Threads
     *  nicht kann. Vor dem Aufbau des Modells gerufen. */
    public static void require(int threads) {
        if (threads > 1 && WITH_THREADS == null) {
            throw new Unavailable("Engine.seedWithThreads fehlt in diesem Stand");
        }
    }

    /** Der Seed auf {@code threads} Threads, ohne {@code seedWithThreads}
     *  mit einem Thread {@link Engine#seed}. */
    public static void seed(Engine e, Graph g, Map<Id, String> vals, int threads) {
        require(threads);
        if (WITH_THREADS == null) {
            e.seed(g, vals);
            return;
        }
        try {
            WITH_THREADS.invoke(e, g, vals, threads);
        } catch (IllegalAccessException ex) {
            throw new IllegalStateException(ex);
        } catch (InvocationTargetException ex) {
            Throwable cause = ex.getCause();
            if (cause instanceof RuntimeException r) throw r;
            if (cause instanceof Error err) throw err;
            throw new IllegalStateException(cause);
        }
    }
}
