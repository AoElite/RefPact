//! The code generator. It is the one place where unsafe code is allowed.

use crate::ir::{Function, Op};

pub struct Generator {
    buffer: Vec<u8>,
}

impl Generator {
    pub fn new() -> Self {
        Generator { buffer: Vec::new() }
    }

    pub fn emit(&mut self, function: &Function) -> &[u8] {
        for op in &function.ops {
            self.buffer.push(match op {
                Op::Push(_) => 0x68,
                Op::Add => 0x01,
                Op::Call(_) => 0xe8,
                Op::Return => 0xc3,
            });
        }
        &self.buffer
    }

    pub fn first_byte(&self) -> u8 {
        assert!(!self.buffer.is_empty());
        // SAFETY: the buffer is not empty.
        unsafe { *self.buffer.get_unchecked(0) }
    }
}

impl Default for Generator {
    fn default() -> Self {
        Generator::new()
    }
}
