mod inner;

/// Nothing outside `dead` uses `dead`.
pub fn unused() {
    inner::helper();
}
