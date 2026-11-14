use super::beta::Beta;

/// `alpha` uses `beta`, `beta` uses `gamma`, and `gamma` uses `alpha`: a cycle of three.
pub struct Alpha(pub Option<Box<Beta>>);
