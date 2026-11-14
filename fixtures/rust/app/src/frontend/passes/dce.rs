use super::Pass;
use crate::ir::{Function, Op};

pub struct Dce;

impl Pass for Dce {
    fn run(&self, function: &mut Function) {
        if let Some(end) = function.ops.iter().position(|op| *op == Op::Return) {
            function.ops.truncate(end + 1);
        }
    }
}
