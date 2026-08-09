package net.sandrakessler.seesaw.hash;

/**
 * Vendored, allocation-arme BLAKE3-Implementierung (unkeyed hashing,
 * 32-Byte-Output) — der GhostId-Hot-Path des Java-Ports.
 *
 * <p>Vollständig inkl. Chunking (1024-B-Chunks, 64-B-Blöcke) und
 * Binärbaum-Merge, weil der Knotenmengen-Fingerprint Multi-Megabyte-
 * Inputs hasht. Korrektheit wird über die aus seesaw-core gedumpten
 * Golden-Vectors abgesichert ({@code fixtures/ghostid_golden.json},
 * inkl. Block-/Chunk-/Baum-Grenzfälle 63…300000 Bytes).
 *
 * <p>Bewusst KEINE externe Bibliothek (Fairness-Vertrag des
 * Sprach-Faktor-Benchmarks: beide Seiten software-pur; Rust nutzt die
 * blake3-Crate ohne explizites SIMD-Feature-Tuning, Java diesen Port).
 */
public final class Blake3 {

    private static final int[] IV = {
        0x6A09E667, 0xBB67AE85, 0x3C6EF372, 0xA54FF53A,
        0x510E527F, 0x9B05688C, 0x1F83D9AB, 0x5BE0CD19
    };

    private static final int[] MSG_PERMUTATION = {
        2, 6, 3, 10, 7, 0, 4, 13, 1, 11, 12, 5, 9, 14, 15, 8
    };

    private static final int CHUNK_START = 1;
    private static final int CHUNK_END = 2;
    private static final int PARENT = 4;
    private static final int ROOT = 8;

    private static final int BLOCK_LEN = 64;
    private static final int CHUNK_LEN = 1024;

    // ── Streaming-Zustand ────────────────────────────────────────
    /** CV-Stack für den Baum-Merge; 54 Ebenen decken 2^54 Chunks. */
    private final int[][] cvStack = new int[54][8];
    private int cvStackLen = 0;

    private final int[] chunkCv = new int[8];
    private final byte[] block = new byte[BLOCK_LEN];
    private int blockLen = 0;
    private int blocksCompressed = 0;
    private long chunkCounter = 0;

    // Arbeits-Puffer (wiederverwendet, keine Allokationen im Update-Pfad)
    private final int[] blockWords = new int[16];
    private final int[] state = new int[16];
    private final int[] m = new int[16];
    private final int[] scratch = new int[16];

    public Blake3() {
        reset();
    }

    /** Setzt den Hasher auf den Anfangszustand zurück (Wiederverwendung). */
    public void reset() {
        System.arraycopy(IV, 0, chunkCv, 0, 8);
        cvStackLen = 0;
        blockLen = 0;
        blocksCompressed = 0;
        chunkCounter = 0;
    }

    public void update(byte[] input) {
        update(input, 0, input.length);
    }

    public void update(byte[] input, int off, int len) {
        int pos = off;
        int end = off + len;
        while (pos < end) {
            // Chunk voll UND weiterer Input → Chunk abschließen.
            if (blockLen == BLOCK_LEN && blocksCompressed == (CHUNK_LEN / BLOCK_LEN) - 1) {
                // letzter Block dieses Chunks (non-root, da mehr Input folgt)
                compressBlockIntoChunkCv(CHUNK_END);
                pushChunkCv();
                startNewChunk();
            } else if (blockLen == BLOCK_LEN) {
                compressBlockIntoChunkCv(0);
            }
            int take = Math.min(BLOCK_LEN - blockLen, end - pos);
            System.arraycopy(input, pos, block, blockLen, take);
            blockLen += take;
            pos += take;
        }
    }

    /** Finalisiert und schreibt 32 Bytes Output; der Hasher ist danach verbraucht. */
    public void finalize32(byte[] out32) {
        // Root-Bestimmung: der letzte Block des letzten Chunks bzw. der
        // letzte Parent-Merge trägt ROOT.
        if (cvStackLen == 0) {
            // Ein einziger (evtl. leerer) Chunk → Root ist der Block selbst.
            int flags = chunkStartFlag() | CHUNK_END | ROOT;
            compressToOutput(chunkCv, block, blockLen, chunkCounter, flags, out32);
            return;
        }
        // Chunk abschließen (non-root) …
        compressBlockIntoChunkCv(CHUNK_END);
        int[] right = new int[8];
        System.arraycopy(chunkCv, 0, right, 0, 8);
        // … und den Stack zusammenmergen; der LETZTE Merge ist Root.
        while (cvStackLen > 1) {
            right = parentCv(cvStack[--cvStackLen], right, 0);
        }
        byte[] parentBlock = new byte[BLOCK_LEN];
        wordsToBytes(cvStack[0], parentBlock, 0);
        wordsToBytes(right, parentBlock, 32);
        compressToOutput(IV, parentBlock, BLOCK_LEN, 0, PARENT | ROOT, out32);
    }

    /** Convenience: kompletter Hash in einem Rutsch. */
    public static byte[] hash(byte[] input) {
        Blake3 h = new Blake3();
        h.update(input);
        byte[] out = new byte[32];
        h.finalize32(out);
        return out;
    }

    // ── interne Mechanik ─────────────────────────────────────────

    private int chunkStartFlag() {
        return blocksCompressed == 0 ? CHUNK_START : 0;
    }

    private void startNewChunk() {
        chunkCounter++;
        System.arraycopy(IV, 0, chunkCv, 0, 8);
        blockLen = 0;
        blocksCompressed = 0;
    }

    private void compressBlockIntoChunkCv(int extraFlags) {
        int flags = chunkStartFlag() | extraFlags;
        bytesToWords(block, blockLen, blockWords);
        compress(chunkCv, blockWords, chunkCounter, blockLen, flags, state, m, scratch);
        for (int i = 0; i < 8; i++) {
            chunkCv[i] = state[i] ^ state[i + 8];
        }
        blocksCompressed++;
        blockLen = 0;
    }

    /**
     * Chunk-CV in den Baum-Stack schieben; Merge-Tiefe = Anzahl der
     * trailing-one-Bits des (bereits inkrementierten) Chunk-Zählers —
     * Standard-BLAKE3-Baum-Regel.
     */
    private void pushChunkCv() {
        int[] cv = new int[8];
        System.arraycopy(chunkCv, 0, cv, 0, 8);
        long totalChunks = chunkCounter + 1;
        while ((totalChunks & 1) == 0) {
            cv = parentCv(cvStack[--cvStackLen], cv, 0);
            totalChunks >>= 1;
        }
        System.arraycopy(cv, 0, cvStack[cvStackLen++], 0, 8);
    }

    private int[] parentCv(int[] left, int[] right, int extraFlags) {
        byte[] parentBlock = new byte[BLOCK_LEN];
        wordsToBytes(left, parentBlock, 0);
        wordsToBytes(right, parentBlock, 32);
        bytesToWords(parentBlock, BLOCK_LEN, blockWords);
        compress(IV, blockWords, 0, BLOCK_LEN, PARENT | extraFlags, state, m, scratch);
        int[] out = new int[8];
        for (int i = 0; i < 8; i++) {
            out[i] = state[i] ^ state[i + 8];
        }
        return out;
    }

    private void compressToOutput(
            int[] cv, byte[] blockBytes, int len, long counter, int flags, byte[] out32) {
        bytesToWords(blockBytes, len, blockWords);
        compress(cv, blockWords, counter, len, flags, state, m, scratch);
        for (int i = 0; i < 8; i++) {
            int w = state[i] ^ state[i + 8];
            out32[i * 4] = (byte) w;
            out32[i * 4 + 1] = (byte) (w >>> 8);
            out32[i * 4 + 2] = (byte) (w >>> 16);
            out32[i * 4 + 3] = (byte) (w >>> 24);
        }
    }

    private static void bytesToWords(byte[] b, int len, int[] words) {
        for (int i = 0; i < 16; i++) {
            int base = i * 4;
            int w = 0;
            if (base < len) {
                w |= (b[base] & 0xFF);
                if (base + 1 < len) w |= (b[base + 1] & 0xFF) << 8;
                if (base + 2 < len) w |= (b[base + 2] & 0xFF) << 16;
                if (base + 3 < len) w |= (b[base + 3] & 0xFF) << 24;
            }
            words[i] = w;
        }
    }

    private static void wordsToBytes(int[] words, byte[] out, int off) {
        for (int i = 0; i < 8; i++) {
            int w = words[i];
            out[off + i * 4] = (byte) w;
            out[off + i * 4 + 1] = (byte) (w >>> 8);
            out[off + i * 4 + 2] = (byte) (w >>> 16);
            out[off + i * 4 + 3] = (byte) (w >>> 24);
        }
    }

    private static void compress(
            int[] cv, int[] blockWords, long counter, int blockLen, int flags,
            int[] state, int[] m, int[] scratch) {
        state[0] = cv[0]; state[1] = cv[1]; state[2] = cv[2]; state[3] = cv[3];
        state[4] = cv[4]; state[5] = cv[5]; state[6] = cv[6]; state[7] = cv[7];
        state[8] = IV[0]; state[9] = IV[1]; state[10] = IV[2]; state[11] = IV[3];
        state[12] = (int) counter;
        state[13] = (int) (counter >>> 32);
        state[14] = blockLen;
        state[15] = flags;

        System.arraycopy(blockWords, 0, m, 0, 16);
        for (int round = 0; round < 7; round++) {
            // Spalten
            g(state, 0, 4, 8, 12, m[0], m[1]);
            g(state, 1, 5, 9, 13, m[2], m[3]);
            g(state, 2, 6, 10, 14, m[4], m[5]);
            g(state, 3, 7, 11, 15, m[6], m[7]);
            // Diagonalen
            g(state, 0, 5, 10, 15, m[8], m[9]);
            g(state, 1, 6, 11, 12, m[10], m[11]);
            g(state, 2, 7, 8, 13, m[12], m[13]);
            g(state, 3, 4, 9, 14, m[14], m[15]);
            if (round < 6) {
                for (int i = 0; i < 16; i++) {
                    scratch[i] = m[MSG_PERMUTATION[i]];
                }
                System.arraycopy(scratch, 0, m, 0, 16);
            }
        }
    }

    private static void g(int[] s, int a, int b, int c, int d, int mx, int my) {
        s[a] = s[a] + s[b] + mx;
        s[d] = Integer.rotateRight(s[d] ^ s[a], 16);
        s[c] = s[c] + s[d];
        s[b] = Integer.rotateRight(s[b] ^ s[c], 12);
        s[a] = s[a] + s[b] + my;
        s[d] = Integer.rotateRight(s[d] ^ s[a], 8);
        s[c] = s[c] + s[d];
        s[b] = Integer.rotateRight(s[b] ^ s[c], 7);
    }
}
