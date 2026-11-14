pub struct Leaf;

#[cfg(test)]
mod tests {
    use super::super::delta::Delta;

    #[test]
    fn a_leaf_is_in_a_delta() {
        let _ = Delta(super::Leaf);
    }
}
