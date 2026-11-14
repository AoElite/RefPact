//! Chooses between the interpreter and the code generator.

use crate::ir::Function;
use crate::codegen::Generator;
use crate::platform::clock::Clock;
use crate::interpreter::Interpreter;

pub fn execute<C: Clock>(function: &Function, clock: C, compile: bool) -> i64 {
    if compile {
        let mut generator = Generator::new();
        return generator.emit(function).len() as i64;
    }
    Interpreter::new(clock).run(function).0
}
