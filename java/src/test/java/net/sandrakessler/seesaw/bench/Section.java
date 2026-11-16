package net.sandrakessler.seesaw.bench;

import java.lang.management.GarbageCollectorMXBean;
import java.lang.management.ManagementFactory;
import java.lang.management.MemoryMXBean;
import java.util.List;

/**
 * Die Messstrecke eines Laufs, Gegenstück zu {@code measure::section}
 * der Rust-Suite ({@code benches/suite/measure.rs}). Die Zelle ruft
 * {@link #start} und {@link #stop} um das, was gemessen wird.
 * Modellaufbau, Laden der Regeln und Fingerprint liegen außerhalb.
 *
 * <p>Gemessen werden:
 * <ul>
 * <li>Thread-CPU (User und System) des Threads, der die Strecke fährt
 *     ({@code ThreadMXBean.getCurrentThreadCpuTime}). GC und JIT laufen
 *     auf eigenen Threads und sind nicht enthalten.</li>
 * <li>CPU (User und System) des ganzen Prozesses, also aller seiner
 *     Threads, auch GC, JIT und der Threads, auf denen
 *     {@code Engine.seedWithThreads} sucht
 *     ({@code OperatingSystemMXBean.getProcessCpuTime}). Gegenstück zur
 *     CPU des Prozesses, die die Rust-Suite misst (dort nur User). -1, wo
 *     die JVM sie nicht liefert.</li>
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
 * <p>Eine Strecke wird einmal gestartet und einmal beendet. Dazwischen
 * kann sie mit {@link #pause} und {@link #resume} in Stücke geteilt
 * werden (Gegenstück zu {@code measure::Meter}): gezählt wird, was
 * zwischen {@link #start} oder {@link #resume} und dem folgenden
 * {@link #pause} oder {@link #stop} läuft, die Werte sind die Summen
 * der Stücke.
 */
public final class Section {
    private static final com.sun.management.ThreadMXBean THREADS =
            (com.sun.management.ThreadMXBean) ManagementFactory.getThreadMXBean();
    private static final List<GarbageCollectorMXBean> GCS =
            ManagementFactory.getGarbageCollectorMXBeans();
    private static final MemoryMXBean MEMORY = ManagementFactory.getMemoryMXBean();
    private static final com.sun.management.OperatingSystemMXBean OS =
            (com.sun.management.OperatingSystemMXBean) ManagementFactory.getOperatingSystemMXBean();

    private final boolean liveHeap;
    private boolean started, running, stopped;
    /** Wall-Zeit bei {@link #start}, für {@link #elapsedNanos}. */
    private long wallStart;
    /** Stand zu Beginn des laufenden Stücks. */
    private long wall0, cpu0, alloc0, gcMillis0, gcCount0, proc0;
    /** Summen der beendeten Stücke. {@code procNanos} -1, sobald ein
     *  Stück die CPU des Prozesses nicht lesen konnte. */
    private long wallNanos, cpuNanos, allocBytes, gcMillis, gcCount, procNanos;
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
        begin();
        wallStart = wall0;
    }

    /** Beendet das laufende Stück. Was bis {@link #resume} läuft, zählt
     *  nicht. */
    public void pause() {
        long[] now = now();
        if (!running) throw new IllegalStateException("Messstrecke läuft nicht");
        end(now);
    }

    /** Beginnt das nächste Stück nach {@link #pause}. */
    public void resume() {
        if (!started || running || stopped) {
            throw new IllegalStateException("Messstrecke nicht unterbrochen");
        }
        begin();
    }

    /** Beendet die Strecke, auch aus der Unterbrechung heraus. */
    public void stop() {
        long[] now = now();
        if (!started || stopped) throw new IllegalStateException("Messstrecke nicht gestartet");
        if (running) end(now);
        stopped = true;
        if (liveHeap) {
            System.gc();
            heapBytes = MEMORY.getHeapMemoryUsage().getUsed();
        }
    }

    private void begin() {
        running = true;
        gcMillis0 = gcMillisNow();
        gcCount0 = gcCountNow();
        alloc0 = THREADS.getCurrentThreadAllocatedBytes();
        proc0 = OS.getProcessCpuTime();
        cpu0 = THREADS.getCurrentThreadCpuTime();
        wall0 = System.nanoTime();
    }

    /** Wall, CPU, Allokation, GC-Zeit, GC-Läufe, CPU des Prozesses, in
     *  umgekehrter Reihenfolge von {@link #begin} gelesen. */
    private static long[] now() {
        long wall = System.nanoTime();
        long cpu = THREADS.getCurrentThreadCpuTime();
        long proc = OS.getProcessCpuTime();
        long alloc = THREADS.getCurrentThreadAllocatedBytes();
        long ms = gcMillisNow();
        long n = gcCountNow();
        return new long[] {wall, cpu, alloc, ms, n, proc};
    }

    private void end(long[] now) {
        running = false;
        wallNanos += now[0] - wall0;
        cpuNanos += now[1] - cpu0;
        allocBytes += now[2] - alloc0;
        gcMillis += now[3] - gcMillis0;
        gcCount += now[4] - gcCount0;
        if (procNanos >= 0) procNanos = proc0 < 0 || now[5] < 0 ? -1 : procNanos + now[5] - proc0;
    }

    /** Wall-Zeit seit {@link #start}, Unterbrechungen eingeschlossen,
     *  für eine Teilzeit in der Strecke. */
    public long elapsedNanos() { return System.nanoTime() - wallStart; }

    public long wallNanos() { return wallNanos; }

    public long cpuNanos() { return cpuNanos; }

    /** CPU des ganzen Prozesses in der Strecke, -1, wo die JVM sie nicht
     *  liefert. */
    public long procCpuNanos() { return procNanos; }

    public long allocBytes() { return allocBytes; }

    public long gcMillis() { return gcMillis; }

    public long gcCount() { return gcCount; }

    /** Belegter Heap nach vollem GC am Ende der Strecke, -1 ohne
     *  {@code liveHeap}. */
    public long heapBytes() { return heapBytes; }
}
