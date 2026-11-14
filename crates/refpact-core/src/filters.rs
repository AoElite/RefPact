//! The filters of a rule (checks 2 to 4 of SPEC 4.2).

use crate::model::{Candidates, Certainty, KindFilter, Reference};

/// The name and the filters of the rule that is evaluated.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Filters<'a> {
    pub(crate) name: &'a str,
    pub(crate) ignore_tags: &'a [String],
    pub(crate) kinds: &'a KindFilter,
    pub(crate) candidates: Candidates,
}

impl Filters<'_> {
    /// Check 2 of SPEC 4.2: one of the tags is one that the rule ignores.
    pub(crate) fn ignores(&self, tags: &[String]) -> bool {
        tags.iter().any(|tag| self.ignore_tags.contains(tag))
    }

    /// Checks 3 and 4 of SPEC 4.2: the kind and the certainty of the reference pass.
    pub(crate) fn admits(&self, reference: &Reference) -> bool {
        let kind_passes = match self.kinds {
            KindFilter::All => true,
            KindFilter::Only(kinds) => kinds.contains(&reference.kind),
            KindFilter::Except(kinds) => !kinds.contains(&reference.kind),
        };
        kind_passes
            && (reference.certainty == Certainty::Exact || self.candidates == Candidates::Include)
    }
}
