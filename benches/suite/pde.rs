//! Workload `pde`: the calls a host made, replayed.
//!
//! The synthetic workloads build their own models. This one takes a
//! record of what a real host did with the public interface of the
//! crate: every call it made to the graph, the value store, the rule
//! loader, the anchor index and its engines, in order, with the
//! arguments and what came back. The cells run the same calls against
//! this build and check each result against the record, so a cell
//! measures this engine on the host's rules and model and fails when
//! the engine answers differently.
//!
//! The record comes from a copy of the host built against a copy of
//! this crate that writes one line per outermost call (calls the crate
//! makes itself, such as the nodes a step creates, are not in it; the
//! replay produces them by making the same call). The records are not
//! part of this repository. `SEESAW_PDE_DUMP` names the directory that
//! holds them, one `<name>.trace` each; every record gives three cells:
//!
//! - `pde/load/<name>`: from the start to the last engine constructed
//!   before the first call on an engine: rule files loaded, anchor
//!   indexes built.
//! - `pde/graph/<name>`: from there to the first mark (or the end),
//!   the calls of the host on graph and value store: the host puts its
//!   model into the graph.
//! - `pde/open/<name>`: the same stretch, the calls on the engines:
//!   the waves. Host and engines alternate there; each of the two
//!   cells counts the pieces of its kind.
//! - `pde/edit/<name>`, for records named `<name>-edit`: everything
//!   from the first mark to the end. The host sets a mark before each
//!   edit it makes after opening.
//!
//! All cells of a record run the whole record; they differ in what is
//! measured, so steps, nodes and fingerprint are the same.
//!
//! ## Format of a record
//!
//! One call per line, fields separated by one space. Strings carry
//! `\s` for a space, `\n`, `\r`, `\t`, `\\`, and `\e` alone is the
//! empty string. An id is `@<hex>` where it first appears, which gives
//! it the next number from 0, and `#<number>` afterwards. Rules are
//! numbered in the order the loader returned them; a sequence of rules
//! is a list of runs `<first>+<count>` separated by commas, `-` when
//! empty. `G`, `V`, `I`, `E` are the numbers of a graph, a value store,
//! an anchor index and an engine. `C N` at the end of an engine call
//! are the length of its cascade and the node count of the graph after
//! the call.
//!
//! ```text
//! gnew G                          Graph::new
//! gclone G G'                     G' = G.clone()
//! intern G name                   G.types.intern(name)
//! base G type external id         G.add_baseline(external, type) = id
//! conn G a b Status id|-          G.connect(a, b, Status)
//! nstat G id Status 0|1           G.set_node_status(id, Status)
//! cstat G id Status 0|1           G.set_connection_status(id, Status)
//! vnew V                          ValueStore::default
//! vclone V V'                     V' = V.clone()
//! val V id value                  V.insert(id, value)
//! load G first count json         rules::load_file(json, G): count rules from number first
//! ibuild I rules                  AnchorIndex::build(rules)
//! enew E rules                    Engine::new(rules)
//! enewidx E rules I               Engine::with_anchor_index(rules, I); I = 0: AnchorIndex::default()
//! contra E 0|1                    E.saw_contradiction = ..
//! active E none|bits              E.active = ..
//! admitd E letters                E.admit_delta (S source, T target)
//! admiti E                        E.admit_initial()
//! added E G V n id.. C N          E.elements_added(G, V, ids)
//! step E G V T|F|N C N            E.step(G, V) = Some(true) | Some(false) | None
//! changed E G id C N              E.element_changed(G, id)
//! deleted E G id C N              E.element_deleted(G, id)
//! consol E G C N                  E.consolidate(G)
//! linkrm E G a b C N              E.link_removed(G, a, b)
//! linkadd E G V a b C N           E.link_added(G, V, a, b)
//! seed E G V C N                  E.seed(G, V)
//! removed E id                    E.element_removed(id)
//! mark name                       the host marks the start of an edit
//! unsupported what                a call the replay does not cover
//! retract E G id C N              E.retract_for(G, id): deprecated, not covered
//! ```

use std::path::{Path, PathBuf};

use seesaw_tgg::engine::{AnchorIndex, DeltaDomain, Engine};
use seesaw_tgg::graph::{Graph, ValueStore};
use seesaw_tgg::ident::{GhostId, Status};
use seesaw_tgg::plan::DirectedRule;
use seesaw_tgg::rules::format::RuleFile;

use crate::{fingerprint, measure, Outcome};

/// Environment variable with the directory of the records.
pub const DUMP: &str = "SEESAW_PDE_DUMP";

/// Which part of a record a cell measures.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Part {
    Load,
    Graph,
    Open,
    Edit,
}

impl Part {
    pub const ALL: [(Part, &'static str, &'static str); 3] = [
        (
            Part::Load,
            "load",
            "host record: rule files loaded, indexes built",
        ),
        (
            Part::Graph,
            "graph",
            "host record: model put into the graph",
        ),
        (Part::Open, "open", "host record: waves of the engines"),
    ];

    /// The part after the first mark, for records named `<name>-edit`.
    pub const EDIT: (Part, &'static str, &'static str) = (
        Part::Edit,
        "edit",
        "host record: waves after the first mark (edits)",
    );
}

/// The records in the directory `SEESAW_PDE_DUMP` names: (name, path),
/// sorted by name. Empty when the variable is not set.
pub fn records() -> Vec<(String, PathBuf)> {
    let Ok(dir) = std::env::var(DUMP) else {
        return Vec::new();
    };
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{DUMP}={dir}: {e}"));
    let mut out: Vec<(String, PathBuf)> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "trace"))
        .filter_map(|p| Some((p.file_stem()?.to_str()?.to_string(), p)))
        .collect();
    out.sort();
    out
}

/// A sequence of rules as runs (first, count).
type Runs = Vec<(u32, u32)>;

/// One recorded call. Ids are indexes into [`Record::ids`].
enum Op {
    GraphNew(u32),
    GraphClone(u32, u32),
    Intern(u32, Box<str>),
    Base {
        g: u32,
        typ: u32,
        external: Box<str>,
        id: u32,
    },
    Connect {
        g: u32,
        a: u32,
        b: u32,
        status: Status,
        id: Option<u32>,
    },
    NodeStatus {
        g: u32,
        id: u32,
        status: Status,
        done: bool,
    },
    ConnectionStatus {
        g: u32,
        id: u32,
        status: Status,
        done: bool,
    },
    StoreNew(u32),
    StoreClone(u32, u32),
    Value {
        v: u32,
        id: u32,
        value: Box<str>,
    },
    Load {
        g: u32,
        first: u32,
        count: u32,
        json: Box<str>,
    },
    IndexBuild {
        i: u32,
        rules: Runs,
    },
    EngineNew {
        e: u32,
        rules: Runs,
        index: Option<u32>,
    },
    Contradiction(u32, bool),
    Active(u32, Option<Box<[bool]>>),
    AdmitDelta(u32, Box<[DeltaDomain]>),
    AdmitInitial(u32),
    /// A mark the host set: the edits begin.
    Mark,
    Added {
        e: u32,
        g: u32,
        v: u32,
        /// Range of [`Record::lists`].
        ids: (u32, u32),
        after: After,
    },
    Step {
        e: u32,
        g: u32,
        v: u32,
        result: Option<bool>,
        after: After,
    },
    Changed {
        e: u32,
        g: u32,
        id: u32,
        after: After,
    },
    Deleted {
        e: u32,
        g: u32,
        id: u32,
        after: After,
    },
    Consolidate {
        e: u32,
        g: u32,
        after: After,
    },
    LinkRemoved {
        e: u32,
        g: u32,
        a: u32,
        b: u32,
        after: After,
    },
    LinkAdded {
        e: u32,
        g: u32,
        v: u32,
        a: u32,
        b: u32,
        after: After,
    },
    Seed {
        e: u32,
        g: u32,
        v: u32,
        after: After,
    },
    Removed(u32, u32),
}

/// Length of the cascade and node count of the graph after a call.
#[derive(Clone, Copy)]
struct After {
    cascade: u32,
    nodes: u32,
}

impl Op {
    /// A call on an engine that exists (not its construction).
    fn is_wave(&self) -> bool {
        !matches!(
            self,
            Op::GraphNew(_)
                | Op::GraphClone(..)
                | Op::Intern(..)
                | Op::Base { .. }
                | Op::Connect { .. }
                | Op::NodeStatus { .. }
                | Op::ConnectionStatus { .. }
                | Op::StoreNew(_)
                | Op::StoreClone(..)
                | Op::Value { .. }
                | Op::Load { .. }
                | Op::IndexBuild { .. }
                | Op::EngineNew { .. }
                | Op::Mark
        )
    }

    /// A call of the host on a graph or a value store.
    fn is_host(&self) -> bool {
        matches!(
            self,
            Op::GraphNew(_)
                | Op::GraphClone(..)
                | Op::Intern(..)
                | Op::Base { .. }
                | Op::Connect { .. }
                | Op::NodeStatus { .. }
                | Op::ConnectionStatus { .. }
                | Op::StoreNew(_)
                | Op::StoreClone(..)
                | Op::Value { .. }
        )
    }

    fn is_setup(&self) -> bool {
        matches!(
            self,
            Op::Load { .. } | Op::IndexBuild { .. } | Op::EngineNew { .. }
        )
    }
}

/// A parsed record.
struct Record {
    ids: Vec<GhostId>,
    types: Vec<Box<str>>,
    lists: Vec<u32>,
    ops: Vec<Op>,
}

fn unescape(s: &str) -> String {
    if s == "\\e" {
        return String::new();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('s') => out.push(' '),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('\\') => out.push('\\'),
            other => panic!("escape \\{other:?} in a record"),
        }
    }
    out
}

fn hex_id(hex: &str) -> GhostId {
    assert_eq!(hex.len(), 64, "id {hex}");
    let mut bytes = [0u8; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).unwrap_or_else(|_| panic!("id {hex}"));
    }
    GhostId::from_raw(bytes)
}

fn status(s: &str) -> Status {
    match s {
        "Solid" => Status::Solid,
        "Ghost" => Status::Ghost,
        "TentativeTombstone" => Status::TentativeTombstone,
        "Tombstone" => Status::Tombstone,
        other => panic!("status {other} in a record"),
    }
}

fn runs(s: &str) -> Runs {
    if s == "-" {
        return Vec::new();
    }
    s.split(',')
        .map(|r| {
            let (a, n) = r.split_once('+').unwrap_or_else(|| panic!("run {r}"));
            (num(a), num(n))
        })
        .collect()
}

fn num(s: &str) -> u32 {
    s.parse()
        .unwrap_or_else(|_| panic!("number {s} in a record"))
}

impl Record {
    fn read(path: &Path) -> Record {
        let text =
            std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut r = Record {
            ids: Vec::new(),
            types: Vec::new(),
            lists: Vec::new(),
            ops: Vec::new(),
        };
        let mut type_ix: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
        for (no, line) in text.lines().enumerate() {
            let mut f = line.split(' ');
            let mut next = || {
                f.next()
                    .unwrap_or_else(|| panic!("{}:{}: field missing", path.display(), no + 1))
            };
            let kind = next();
            // An id: defined where it first appears.
            macro_rules! id {
                () => {{
                    let t = next();
                    match t.as_bytes()[0] {
                        b'@' => {
                            r.ids.push(hex_id(&t[1..]));
                            r.ids.len() as u32 - 1
                        }
                        b'#' => num(&t[1..]),
                        _ => panic!("{}:{}: id {t}", path.display(), no + 1),
                    }
                }};
            }
            macro_rules! after {
                () => {
                    After {
                        cascade: num(next()),
                        nodes: num(next()),
                    }
                };
            }
            let op = match kind {
                "gnew" => Op::GraphNew(num(next())),
                "gclone" => Op::GraphClone(num(next()), num(next())),
                "intern" => Op::Intern(num(next()), unescape(next()).into()),
                "base" => {
                    let g = num(next());
                    let name = unescape(next());
                    let typ = *type_ix.entry(name.clone()).or_insert_with(|| {
                        r.types.push(name.into());
                        r.types.len() as u32 - 1
                    });
                    let external = unescape(next()).into();
                    Op::Base {
                        g,
                        typ,
                        external,
                        id: id!(),
                    }
                }
                "conn" => {
                    let g = num(next());
                    let a = id!();
                    let b = id!();
                    let status = status(next());
                    let t = next();
                    let id = match t.as_bytes()[0] {
                        b'-' => None,
                        b'@' => {
                            r.ids.push(hex_id(&t[1..]));
                            Some(r.ids.len() as u32 - 1)
                        }
                        _ => Some(num(&t[1..])),
                    };
                    Op::Connect {
                        g,
                        a,
                        b,
                        status,
                        id,
                    }
                }
                "nstat" => Op::NodeStatus {
                    g: num(next()),
                    id: id!(),
                    status: status(next()),
                    done: next() == "1",
                },
                "cstat" => Op::ConnectionStatus {
                    g: num(next()),
                    id: id!(),
                    status: status(next()),
                    done: next() == "1",
                },
                "vnew" => Op::StoreNew(num(next())),
                "vclone" => Op::StoreClone(num(next()), num(next())),
                "val" => Op::Value {
                    v: num(next()),
                    id: id!(),
                    value: unescape(next()).into(),
                },
                "load" => Op::Load {
                    g: num(next()),
                    first: num(next()),
                    count: num(next()),
                    json: unescape(next()).into(),
                },
                "ibuild" => Op::IndexBuild {
                    i: num(next()),
                    rules: runs(next()),
                },
                "enew" => Op::EngineNew {
                    e: num(next()),
                    rules: runs(next()),
                    index: None,
                },
                "enewidx" => Op::EngineNew {
                    e: num(next()),
                    rules: runs(next()),
                    index: Some(num(next())),
                },
                "contra" => Op::Contradiction(num(next()), next() == "1"),
                "active" => {
                    let e = num(next());
                    let bits = next();
                    let mask = (bits != "none").then(|| bits.bytes().map(|b| b == b'1').collect());
                    Op::Active(e, mask)
                }
                "admitd" => {
                    let e = num(next());
                    let domains = unescape(next())
                        .chars()
                        .map(|c| match c {
                            'S' => DeltaDomain::Source,
                            'T' => DeltaDomain::Target,
                            other => panic!("delta domain {other}"),
                        })
                        .collect();
                    Op::AdmitDelta(e, domains)
                }
                "admiti" => Op::AdmitInitial(num(next())),
                "mark" => Op::Mark,
                "added" => {
                    let (e, g, v) = (num(next()), num(next()), num(next()));
                    let n = num(next());
                    let start = r.lists.len() as u32;
                    for _ in 0..n {
                        let id = id!();
                        r.lists.push(id);
                    }
                    Op::Added {
                        e,
                        g,
                        v,
                        ids: (start, n),
                        after: after!(),
                    }
                }
                "step" => Op::Step {
                    e: num(next()),
                    g: num(next()),
                    v: num(next()),
                    result: match next() {
                        "T" => Some(true),
                        "F" => Some(false),
                        "N" => None,
                        other => panic!("step result {other}"),
                    },
                    after: after!(),
                },
                "changed" => Op::Changed {
                    e: num(next()),
                    g: num(next()),
                    id: id!(),
                    after: after!(),
                },
                "deleted" => Op::Deleted {
                    e: num(next()),
                    g: num(next()),
                    id: id!(),
                    after: after!(),
                },
                "consol" => Op::Consolidate {
                    e: num(next()),
                    g: num(next()),
                    after: after!(),
                },
                "linkrm" => Op::LinkRemoved {
                    e: num(next()),
                    g: num(next()),
                    a: id!(),
                    b: id!(),
                    after: after!(),
                },
                "linkadd" => Op::LinkAdded {
                    e: num(next()),
                    g: num(next()),
                    v: num(next()),
                    a: id!(),
                    b: id!(),
                    after: after!(),
                },
                "seed" => Op::Seed {
                    e: num(next()),
                    g: num(next()),
                    v: num(next()),
                    after: after!(),
                },
                "removed" => Op::Removed(num(next()), id!()),
                "unsupported" | "retract" => panic!(
                    "{}:{}: the host made a call the replay does not cover: {line}",
                    path.display(),
                    no + 1
                ),
                other => panic!("{}:{}: unknown call {other}", path.display(), no + 1),
            };
            r.ops.push(op);
        }
        r
    }

    /// Where the load part ends and where the edits begin: after the
    /// last rule file loaded, index built or engine constructed before
    /// the first call on an engine, and at the first mark (the end of
    /// the record without one).
    fn bounds(&self) -> (usize, usize) {
        let end = self.ops.len();
        let wave = self.ops.iter().position(Op::is_wave).unwrap_or(end);
        let load = self.ops[..wave]
            .iter()
            .rposition(Op::is_setup)
            .map_or(0, |i| i + 1);
        let mark = self
            .ops
            .iter()
            .position(|op| matches!(op, Op::Mark))
            .unwrap_or(end);
        (load, mark)
    }
}

/// What the replay holds: everything the host held.
#[derive(Default)]
struct World {
    graphs: Vec<Option<Graph>>,
    stores: Vec<Option<ValueStore>>,
    /// All rules in the order the loader returned them. Allocated once
    /// with room for every rule of the record and never freed: the
    /// engines and indexes hold slices of it, as the host's engines
    /// hold slices of its rule sets.
    rules: std::mem::ManuallyDrop<Vec<DirectedRule>>,
    indexes: Vec<Option<&'static AnchorIndex>>,
    engines: Vec<Option<Engine<'static>>>,
    /// Engines in the order of their construction.
    order: Vec<u32>,
    /// Graph and store of the last engine call.
    last: Option<(u32, u32)>,
    steps: u64,
}

fn slot<T>(v: &mut Vec<Option<T>>, ix: u32) -> &mut Option<T> {
    let ix = ix as usize;
    if v.len() <= ix {
        v.resize_with(ix + 1, || None);
    }
    &mut v[ix]
}

fn have<'a, T>(v: &'a mut [Option<T>], ix: u32, what: &str) -> &'a mut T {
    v.get_mut(ix as usize)
        .and_then(Option::as_mut)
        .unwrap_or_else(|| panic!("the record uses {what} {ix} before it exists"))
}

impl World {
    fn slice(&self, runs: &Runs) -> &'static [DirectedRule] {
        if let [(first, count)] = runs[..] {
            let (first, count) = (first as usize, count as usize);
            assert!(first + count <= self.rules.len(), "rules of the record");
            // SAFETY: the elements are initialised. `rules` was
            // allocated with the capacity for all rules of the record
            // (checked where rules are appended), so it is never moved,
            // and it is never freed or shrunk. Appending writes behind
            // the elements handed out here.
            return unsafe { std::slice::from_raw_parts(self.rules.as_ptr().add(first), count) };
        }
        // Rules that are not one run: a copy.
        let mut out = Vec::new();
        for (first, count) in runs {
            out.extend_from_slice(&self.rules[*first as usize..(*first + *count) as usize]);
        }
        Box::leak(out.into_boxed_slice())
    }

    /// Checks the cascade and the graph against the record.
    fn check(&mut self, no: usize, e: u32, g: u32, after: After) {
        let cascade = have(&mut self.engines, e, "engine").cascade.len();
        let nodes = have(&mut self.graphs, g, "graph").node_count();
        assert!(
            cascade == after.cascade as usize && nodes == after.nodes as usize,
            "call {no} of the record: cascade {cascade}, nodes {nodes}; \
             the host had cascade {}, nodes {}",
            after.cascade,
            after.nodes
        );
    }

    fn run(&mut self, r: &Record, range: std::ops::Range<usize>) {
        for no in range {
            match &r.ops[no] {
                Op::GraphNew(g) => *slot(&mut self.graphs, *g) = Some(Graph::new()),
                Op::GraphClone(from, to) => {
                    let copy = have(&mut self.graphs, *from, "graph").clone();
                    *slot(&mut self.graphs, *to) = Some(copy);
                }
                Op::Intern(g, name) => {
                    have(&mut self.graphs, *g, "graph").types.intern(name);
                }
                Op::Base {
                    g,
                    typ,
                    external,
                    id,
                } => {
                    let got = have(&mut self.graphs, *g, "graph")
                        .add_baseline(external, &r.types[*typ as usize]);
                    assert!(got == r.ids[*id as usize], "call {no}: add_baseline");
                }
                Op::Connect {
                    g,
                    a,
                    b,
                    status,
                    id,
                } => {
                    let got = have(&mut self.graphs, *g, "graph").connect(
                        r.ids[*a as usize],
                        r.ids[*b as usize],
                        *status,
                    );
                    assert!(got == id.map(|i| r.ids[i as usize]), "call {no}: connect");
                }
                Op::NodeStatus {
                    g,
                    id,
                    status,
                    done,
                } => {
                    let got = have(&mut self.graphs, *g, "graph")
                        .set_node_status(&r.ids[*id as usize], *status);
                    assert!(got == *done, "call {no}: set_node_status");
                }
                Op::ConnectionStatus {
                    g,
                    id,
                    status,
                    done,
                } => {
                    let got = have(&mut self.graphs, *g, "graph")
                        .set_connection_status(&r.ids[*id as usize], *status);
                    assert!(got == *done, "call {no}: set_connection_status");
                }
                Op::StoreNew(v) => *slot(&mut self.stores, *v) = Some(ValueStore::default()),
                Op::StoreClone(from, to) => {
                    let copy = have(&mut self.stores, *from, "store").clone();
                    *slot(&mut self.stores, *to) = Some(copy);
                }
                Op::Value { v, id, value } => {
                    have(&mut self.stores, *v, "store")
                        .insert(r.ids[*id as usize], value.to_string());
                }
                Op::Load {
                    g,
                    first,
                    count,
                    json,
                } => {
                    let file = RuleFile::from_json(json)
                        .unwrap_or_else(|e| panic!("call {no}: rule file: {e}"));
                    let rules =
                        seesaw_tgg::rules::load_file(&file, have(&mut self.graphs, *g, "graph"))
                            .unwrap_or_else(|e| panic!("call {no}: rule file: {e}"));
                    assert!(
                        rules.len() == *count as usize && self.rules.len() == *first as usize,
                        "call {no}: {} rules from number {}, the host had {count} from {first}",
                        rules.len(),
                        self.rules.len()
                    );
                    assert!(
                        self.rules.len() + rules.len() <= self.rules.capacity(),
                        "call {no}: more rules than the record announced"
                    );
                    self.rules.extend(rules);
                }
                Op::IndexBuild { i, rules } => {
                    let index: &'static AnchorIndex =
                        Box::leak(Box::new(AnchorIndex::build(self.slice(rules))));
                    *slot(&mut self.indexes, *i) = Some(index);
                }
                Op::EngineNew { e, rules, index } => {
                    let rules = self.slice(rules);
                    let engine = match index {
                        None => Engine::new(rules),
                        Some(0) => Engine::with_anchor_index(
                            rules,
                            Box::leak(Box::new(AnchorIndex::default())),
                        ),
                        Some(i) => {
                            Engine::with_anchor_index(rules, have(&mut self.indexes, *i, "index"))
                        }
                    };
                    *slot(&mut self.engines, *e) = Some(engine);
                    self.order.push(*e);
                }
                Op::Contradiction(e, value) => {
                    have(&mut self.engines, *e, "engine").saw_contradiction = *value;
                }
                Op::Active(e, mask) => {
                    have(&mut self.engines, *e, "engine").active =
                        mask.as_ref().map(|m| m.to_vec());
                }
                Op::AdmitDelta(e, domains) => {
                    have(&mut self.engines, *e, "engine").admit_delta(domains);
                }
                Op::AdmitInitial(e) => have(&mut self.engines, *e, "engine").admit_initial(),
                Op::Mark => {}
                Op::Added {
                    e,
                    g,
                    v,
                    ids,
                    after,
                } => {
                    let nodes: Vec<GhostId> = r.lists[ids.0 as usize..(ids.0 + ids.1) as usize]
                        .iter()
                        .map(|i| r.ids[*i as usize])
                        .collect();
                    have(&mut self.engines, *e, "engine").elements_added(
                        have(&mut self.graphs, *g, "graph"),
                        have(&mut self.stores, *v, "store"),
                        &nodes,
                    );
                    self.last = Some((*g, *v));
                    self.check(no, *e, *g, *after);
                }
                Op::Step {
                    e,
                    g,
                    v,
                    result,
                    after,
                } => {
                    let got = have(&mut self.engines, *e, "engine").step(
                        have(&mut self.graphs, *g, "graph"),
                        have(&mut self.stores, *v, "store"),
                    );
                    assert!(
                        got == *result,
                        "call {no}: step returned {got:?}, the host had {result:?}"
                    );
                    self.steps += u64::from(got.is_some());
                    self.last = Some((*g, *v));
                    self.check(no, *e, *g, *after);
                }
                Op::Changed { e, g, id, after } => {
                    have(&mut self.engines, *e, "engine")
                        .element_changed(have(&mut self.graphs, *g, "graph"), &r.ids[*id as usize]);
                    self.check(no, *e, *g, *after);
                }
                Op::Deleted { e, g, id, after } => {
                    have(&mut self.engines, *e, "engine")
                        .element_deleted(have(&mut self.graphs, *g, "graph"), &r.ids[*id as usize]);
                    self.check(no, *e, *g, *after);
                }
                Op::Consolidate { e, g, after } => {
                    have(&mut self.engines, *e, "engine").consolidate(have(
                        &mut self.graphs,
                        *g,
                        "graph",
                    ));
                    self.check(no, *e, *g, *after);
                }
                Op::LinkRemoved { e, g, a, b, after } => {
                    have(&mut self.engines, *e, "engine").link_removed(
                        have(&mut self.graphs, *g, "graph"),
                        &r.ids[*a as usize],
                        &r.ids[*b as usize],
                    );
                    self.check(no, *e, *g, *after);
                }
                Op::LinkAdded {
                    e,
                    g,
                    v,
                    a,
                    b,
                    after,
                } => {
                    have(&mut self.engines, *e, "engine").link_added(
                        have(&mut self.graphs, *g, "graph"),
                        have(&mut self.stores, *v, "store"),
                        &r.ids[*a as usize],
                        &r.ids[*b as usize],
                    );
                    self.last = Some((*g, *v));
                    self.check(no, *e, *g, *after);
                }
                Op::Seed { e, g, v, after } => {
                    have(&mut self.engines, *e, "engine").seed(
                        have(&mut self.graphs, *g, "graph"),
                        have(&mut self.stores, *v, "store"),
                    );
                    self.last = Some((*g, *v));
                    self.check(no, *e, *g, *after);
                }
                Op::Removed(e, id) => {
                    have(&mut self.engines, *e, "engine").element_removed(&r.ids[*id as usize]);
                }
            }
        }
    }
}

/// Replays the record at `path` and measures `part` of it.
pub fn replay(path: &Path, part: Part) -> Outcome {
    let record = Record::read(path);
    let (load, mark) = record.bounds();
    let end = record.ops.len();
    assert!(
        record.ops.iter().any(Op::is_wave),
        "{}: the record holds no call on an engine",
        path.display()
    );
    assert!(
        part != Part::Edit || mark < end,
        "{}: the record holds no mark",
        path.display()
    );
    let mut world = World::default();
    let rules: usize = record
        .ops
        .iter()
        .map(|op| match op {
            Op::Load { count, .. } => *count as usize,
            _ => 0,
        })
        .sum();
    world.rules.reserve_exact(rules);
    let mut meter = measure::Meter::start();
    let mut piece = |world: &mut World, range: std::ops::Range<usize>, measured: bool| {
        if measured {
            meter.run(|| world.run(&record, range));
        } else {
            world.run(&record, range);
        }
    };
    piece(&mut world, 0..load, part == Part::Load);
    // Opening: the host's calls on graph and store and the calls on
    // the engines alternate; each cell counts its kind.
    let mut at = load;
    while at < mark {
        let host = record.ops[at].is_host();
        let mut to = at + 1;
        while to < mark && record.ops[to].is_host() == host {
            to += 1;
        }
        let measured = part == if host { Part::Graph } else { Part::Open };
        piece(&mut world, at..to, measured);
        at = to;
    }
    piece(&mut world, mark..end, part == Part::Edit);
    let usage = meter.finish();
    let (g, v) = world.last.unwrap_or_else(|| {
        panic!(
            "{}: no engine call with a graph in the record",
            path.display()
        )
    });
    let g = world.graphs[g as usize]
        .as_ref()
        .expect("graph of the waves");
    let v = world.stores[v as usize]
        .as_ref()
        .expect("store of the waves");
    let engines: Vec<&Engine<'static>> = world
        .order
        .iter()
        .filter_map(|e| world.engines[*e as usize].as_ref())
        .collect();
    Outcome {
        usage,
        steps: world.steps,
        applied: engines.iter().map(|e| e.cascade.len() as u64).sum(),
        nodes: g.node_count() as u64,
        fingerprint: fingerprint::of_all(g, &engines, v),
    }
}
