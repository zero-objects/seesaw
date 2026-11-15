package net.sandrakessler.seesaw.bench;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryMXBean;
import java.util.List;

/**
 * Die Messstrecke eines Laufs, Gegenstück zu {@code measure::section}
 * der Rust-Suite ({@code benches/suite/measure.rs}). Die Zelle ruft
 * {@link #start} und {@link #stop} um das, was gemessen wird;
 * Modellaufbau, Laden der Regeln und Fingerprint liegen außerhalb.
 *
 * <p>Gemessen werden:
 * <ul>
 * <li>Thread-CPU (User und System) des Threads, der die Strecke fährt
 *     ({@code ThreadMXBean.getCurrentThreadCpuTime}). GC und JIT laufen
 *     auf eigenen Threads und sind nicht enthalten.</li>
 * <li>Wall-Zeit ({@code System.nanoTime}).</li>
 * <li>Bytes, die der Thread in der Strecke alloziert
 *     ({@code getCurrentThreadAllocatedBytes}).</li>
 * <li>Zeit (ms) und Zahl der GC-Läufe aller Collectors in der
 *     Strecke ({@code GarbageCollectorMXBean}).</li>
 * <li>Mit {@code liveHeap}: nach dem Ende der Strecke ein voller GC
 *     ({@code System.gc()}), dann der belegte Heap, also was Modell,
 *     Regeln und Engine am Ende halten. Der GC liegt hinter allen
 *     anderen Werten und zählt nicht mit.</li>
 * </ul>
 *
 * <p>Eine Strecke wird einmal gestartet und einmal beendet.
 */
public final class Section {
    private static final com.sun.management.ThreadMXBean THREADS =
            (com.sun.management.ThreadMXBean) ManagementFactory.getThreadMXBean();
    private static final List<GarbageCollectorMXBean> GCS =
            ManagementFactory.getGarbageCollectorMXBeans();
    private static final MemoryMXBean MEMORY = ManagementFactory.getMemoryMXBean();

    private final boolean liveHeap;
    private boolean started, stopped;
    private long wall0, cpu0, alloc0, gcMillis0, gcCount0;
    private long wallNanos, cpuNanos, allocBytes, gcMillis, gcCount;
    private long heapBytes = -1;

    /** Ohne Messung des Heaps. */
    public Section() { this(false); }

    /** {@code liveHeap}: nach der Strecke voller GC und belegter Heap,
     *  siehe {@link #heapBytes}. */
    public Section(boolean liveHeap) { this.liveHeap = liveHeap; }

    private static long gcMillisNow() {
        long sum = 0;
        for (GarbageCollectorMXBean b : GCS) sum += Math.max(0, b.getCollectionTime());
        return sum;
    }

    private static long gcCountNow() {
        long sum = 0;
        for (GarbageCollectorMXBean b : GCS) sum += Math.max(0, b.getCollectionCount());
        return sum;
    }

    public void start() {
        if (started) throw new IllegalStateException("Messstrecke schon gestartet");
        started = true;
        gcMillis0 = gcMillisNow();
        gcCount0 = gcCountNow();
        alloc0 = THREADS.getCurrentThreadAllocatedBytes();
        cpu0 = THREADS.getCurrentThreadCpuTime();
        wall0 = System.nanoTime();
    }

    public void stop() {
        long wall = System.nanoTime();
        long cpu = THREADS.getCurrentThreadCpuTime();
        long alloc = THREADS.getCurrentThreadAllocatedBytes();
        long ms = gcMillisNow();
        long n = gcCountNow();
        if (!started || stopped) throw new IllegalStateException("Messstrecke nicht gestartet");
        stopped = true;
        wallNanos = wall - wall0;
        cpuNanos = cpu - cpu0;
        allocBytes = alloc - alloc0;
        gcMillis = ms - gcMillis0;
        gcCount = n - gcCount0;
        if (liveHeap) {
            System.gc();
            heapBytes = MEMORY.getHeapMemoryUsage().getUsed();
        }
    }

    /** Wall-Zeit seit {@link #start}, für eine Teilzeit in der Strecke. */
    public long elapsedNanos() { return System.nanoTime() - wall0; }

    public long wallNanos() { return wallNanos; }

    public long cpuNanos() { return cpuNanos; }

    public long allocBytes() { return allocBytes; }

    public long gcMillis() { return gcMillis; }

    public long gcCount() { return gcCount; }

    /** Belegter Heap nach vollem GC am Ende der Strecke, -1 ohne
     *  {@code liveHeap}. */
    public long heapBytes() { return heapBytes; }
}
