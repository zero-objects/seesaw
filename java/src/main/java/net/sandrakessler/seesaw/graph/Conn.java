package net.sandrakessler.seesaw.graph;

import net.sandrakessler.seesaw.ident.Id;
import net.sandrakessler.seesaw.ident.St;

public final class Conn {
    public final Id id, source, target;
    public St status;

  public   Conn(Id id, Id s, Id t, St st) { this.id = id; source = s; target = t; status = st; }
}
