//! The interpreter. It must not use the code generator.

pub mod heap;

// Not a violation of rule 3: this `env` is a module of this crate, not `std::env`.
mod env;

use self::env::Frame;
use crate::ir::{Function, Op};
use crate::platform::clock::Clock;

pub struct Interpreter<C: Clock> {
    clock: C,
    frames: Vec<Frame>,
}

impl<C: Clock> Interpreter<C> {
    pub fn new(clock: C) -> Self {
        Interpreter { clock, frames: Vec::new() }
    }

    pub fn run(&mut self, function: &Function) -> (i64, u64) {
        let start = self.clock.now_millis();
        self.frames.push(Frame::default());
        let mut top = 0;
        for op in &function.ops {
            match op {
                Op::Push(value) => top = *value,
                Op::Add => top += 1,
                Op::Call(_) | Op::Return => break,
            }
        }
        top += self.frames.pop().map_or(0, |frame| frame.locals.len() as i64);
        (top, self.clock.now_millis() - start)
    }
}
