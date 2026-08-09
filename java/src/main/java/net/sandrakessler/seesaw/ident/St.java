package net.sandrakessler.seesaw.ident;


// ── Status (Spiegel von ident::Status, nur benötigte Teile) ──
public enum St {
    SOLID,
    GHOST,
    TENTATIVE_TOMBSTONE,
    TOMBSTONE;

    /** Rust Status::is_matchable — TT bleibt matchbar (Resurrektions-Fenster).
     *  Public seit E2 (Session-Codecs in session). */
    public boolean matchable() {
        return this == SOLID || this == GHOST || this == TENTATIVE_TOMBSTONE;
    }

    /** Rust node_alive/connected_alive — nur nicht-tentativ (Solid|Ghost). */
    public boolean alive() { return this == SOLID || this == GHOST; }
}
