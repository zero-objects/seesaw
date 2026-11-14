//! Performance suite of the engine.
//!
//! One cell is one workload with one operation at one size. A cell runs
//! alone in its process and writes one TSV line:
//!
//! ```text
//! cargo bench --bench suite -- list
//! cargo bench --bench suite -- run langpack/add/k700/n2000 --samples 5 --label base
//! cargo bench --bench suite --features perf_trace -- run <cell> --samples 1
//! ```
//!
//! Columns: label, cell, samples, user CPU (minimum and median, ms),
//! wall time (minimum, ms), peak memory (MB), steps, applied steps,
//! nodes, fingerprint, then the structural counters (with
//! `perf_trace`). Time is user CPU of the measured section only, model
//! construction and rule loading are outside of it. The fingerprint
//! covers graph and cascade order and has to be the same for every
//! build of the engine.

mod f2p;
mod fingerprint;
mod langpack;
mod measure;

use std::io::Write;

/// Result of one run of a cell.
pub struct Outcome {
    pub usage: measure::Usage,
    /// `step` calls that returned `Some`.
    pub steps: u64,
    /// Entries of the cascade.
    pub applied: u64,
    /// Nodes of the graph at the end.
    pub nodes: u64,
    pub fingerprint: String,
}

struct Cell {
    name: String,
    about: &'static str,
    run: Box<dyn Fn() -> Outcome>,
}

fn cells() -> Vec<Cell> {
    use langpack::Spec;
    let mut out = Vec::new();
    let mut cell = |name: String, about: &'static str, run: Box<dyn Fn() -> Outcome>| {
        out.push(Cell { name, about, run });
    };
    for (kinds, nodes) in [(50, 2_000), (700, 2_000), (700, 20_000)] {
        let spec = Spec { kinds, nodes };
        let size = format!("k{kinds}/n{nodes}");
        cell(
            format!("langpack/seed/{size}"),
            "batch through seed",
            Box::new(move || langpack::seed(spec)),
        );
        cell(
            format!("langpack/add/{size}"),
            "whole model as one add delta",
            Box::new(move || langpack::add(spec)),
        );
        cell(
            format!("langpack/backward/{size}"),
            "target side as one add delta",
            Box::new(move || langpack::backward(spec)),
        );
        cell(
            format!("langpack/delta-add/{size}/d200"),
            "200 added nodes, one wave each",
            Box::new(move || langpack::delta_add(spec, 200)),
        );
        cell(
            format!("langpack/delta-change/{size}/d200"),
            "200 changed leaf values, one wave each",
            Box::new(move || langpack::delta_change(spec, 200)),
        );
        cell(
            format!("langpack/delta-delete/{size}/d200"),
            "200 deleted nodes, one wave each",
            Box::new(move || langpack::delta_delete(spec, 200)),
        );
    }
    // 200 000 families (the 10^6 anchor of the report) need more than
    // 10 GB at about 2.8 KB per node and are not a cell.
    for families in [2_000, 20_000, 60_000] {
        cell(
            format!("f2p/batch/f{families}"),
            "batch through run (seed and steps)",
            Box::new(move || f2p::batch(families)),
        );
        cell(
            format!("f2p/add/f{families}"),
            "whole model as one add delta",
            Box::new(move || f2p::add(families)),
        );
        cell(
            format!("f2p/delta-add/f{families}/d1000"),
            "1000 added families, one wave each",
            Box::new(move || f2p::delta_add(families, 1000)),
        );
    }
    out
}

fn usage() -> ! {
    eprintln!("usage: suite list | run <cell> [--samples N] [--label L] [--tsv FILE]");
    std::process::exit(2);
}

fn main() {
    // `cargo bench` appends `--bench`.
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--bench")
        .collect();
    let cells = cells();
    match args.first().map(String::as_str) {
        Some("list") => {
            for c in &cells {
                println!("{}\t{}", c.name, c.about);
            }
        }
        Some("run") => {
            let Some(name) = args.get(1) else { usage() };
            let mut samples = 3usize;
            let mut label = String::from("-");
            let mut tsv: Option<String> = None;
            let mut rest = args[2..].iter();
            while let Some(a) = rest.next() {
                let mut value = || rest.next().cloned().unwrap_or_else(|| usage());
                match a.as_str() {
                    "--samples" => samples = value().parse().unwrap_or_else(|_| usage()),
                    "--label" => label = value(),
                    "--tsv" => tsv = Some(value()),
                    _ => usage(),
                }
            }
            let Some(cell) = cells.iter().find(|c| &c.name == name) else {
                eprintln!("unknown cell {name}");
                std::process::exit(2);
            };
            let line = run(cell, samples.max(1), &label);
            println!("{line}");
            if let Some(path) = tsv {
                let mut f = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&path)
                    .unwrap_or_else(|e| panic!("{path}: {e}"));
                writeln!(f, "{line}").unwrap_or_else(|e| panic!("{path}: {e}"));
            }
        }
        _ => usage(),
    }
}

fn run(cell: &Cell, samples: usize, label: &str) -> String {
    let mut outcomes: Vec<Outcome> = (0..samples).map(|_| (cell.run)()).collect();
    let first = &outcomes[0];
    for o in &outcomes {
        assert_eq!(
            o.fingerprint, first.fingerprint,
            "{}: the fingerprint differs between samples",
            cell.name
        );
        assert_eq!(o.usage.counters, first.usage.counters);
    }
    let (steps, applied, nodes) = (first.steps, first.applied, first.nodes);
    let fingerprint = first.fingerprint.clone();
    let counters = first.usage.counters.clone();
    let wall_min = outcomes.iter().map(|o| o.usage.wall_ns).min().unwrap_or(0);
    outcomes.sort_by_key(|o| o.usage.user_ns);
    let user_min = outcomes[0].usage.user_ns;
    let user_med = outcomes[outcomes.len() / 2].usage.user_ns;
    let ms = |ns: u64| format!("{:.1}", ns as f64 / 1e6);
    let mut cols = vec![
        label.to_string(),
        cell.name.clone(),
        samples.to_string(),
        ms(user_min),
        ms(user_med),
        ms(wall_min),
        format!(
            "{:.0}",
            measure::peak_rss_bytes() as f64 / (1024.0 * 1024.0)
        ),
        steps.to_string(),
        applied.to_string(),
        nodes.to_string(),
        fingerprint,
    ];
    for (name, value) in counters {
        cols.push(format!("{name}={value}"));
    }
    cols.join("\t")
}
