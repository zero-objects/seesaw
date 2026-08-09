//! The rule format: declarative, named, purely positive.
//!
//! Spec: paper/implementation-report/specs/2026-08-07-regelformat-v3.md
//!
//! One way leads from a rule to an execution plan, and it has three
//! steps: parse, validate, lower. [`load`] is that way; the modules
//! below are its stages, public so a host can report errors per stage.

pub mod export;
pub mod format;
pub mod lower;
pub mod predicate;
pub mod transform;
pub mod validate;

use crate::graph::Graph;
use crate::plan::DirectedRule;

/// Format version in the header of every file.
pub const FORMAT_VERSION: u32 = 3;

/// Why loading failed, in the stage where it did.
#[derive(Debug)]
pub enum LoadError {
    /// The text is not the JSON this format expects.
    Parse(serde_json::Error),
    /// The file parses but says something inconsistent.
    Validate(validate::LoadError),
    /// The rules are consistent but cannot be lowered.
    Lower(lower::LowerError),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "rule file does not parse: {e}"),
            Self::Validate(e) => write!(f, "rule file does not validate: {e:?}"),
            Self::Lower(e) => write!(f, "rule file does not lower: {e:?}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// A rule file to directed creation plans, two per rule: forward first,
/// then backward, in declaration order.
///
/// Types are interned into `g` along the way, so the graph the rules
/// will run against has to be the one passed here.
pub fn load(json: &str, g: &mut Graph) -> Result<Vec<DirectedRule>, LoadError> {
    let file = format::RuleFile::from_json(json).map_err(LoadError::Parse)?;
    load_file(&file, g)
}

/// Same, from an already parsed file. For hosts that build the file
/// themselves instead of reading text.
pub fn load_file(file: &format::RuleFile, g: &mut Graph) -> Result<Vec<DirectedRule>, LoadError> {
    let resolved = validate::validate(file).map_err(LoadError::Validate)?;
    lower::lower_all(&resolved, g).map_err(LoadError::Lower)
}

/// Only the forward direction, for callers that never run backward.
pub fn load_forward(json: &str, g: &mut Graph) -> Result<Vec<DirectedRule>, LoadError> {
    Ok(load(json, g)?.into_iter().step_by(2).collect())
}
