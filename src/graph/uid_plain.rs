// Which table instance a table is: nothing, without the feature
// `opt_g`. With it `uid_counted.rs` stands here.

/// The identity of one table instance. Empty: nothing asks for it.
#[derive(Debug, Clone, Default)]
pub(crate) struct TableUid;
