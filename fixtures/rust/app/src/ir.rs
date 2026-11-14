//! The intermediate representation that all layers share.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Push(i64),
    Add,
    Call(usize),
    Return,
}

#[derive(Debug, Clone, Default)]
pub struct Function {
    pub name: String,
    pub ops: Vec<Op>,
}

impl Function {
    pub fn new(name: &str) -> Self {
        Function { name: name.to_owned(), ops: Vec::new() }
    }
}
