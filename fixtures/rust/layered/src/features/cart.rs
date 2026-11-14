pub fn total() -> u32 {
    crate::features::shared::round(crate::features::pay::fee())
}
