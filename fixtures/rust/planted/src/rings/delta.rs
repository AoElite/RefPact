use super::leaf::Leaf;

/// `delta` uses `leaf`. `leaf` uses `delta` in its tests only.
pub struct Delta(pub Leaf);
