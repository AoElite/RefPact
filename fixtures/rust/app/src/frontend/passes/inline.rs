use std::collections::HashMap;

use rand::Rng;

use super::Pass;
use crate::ir::{Function, Op};

pub struct Inline {
    pub budget: usize,
}

impl Pass for Inline {
    fn run(&self, function: &mut Function) {
        let mut seen: HashMap<usize, usize> = HashMap::new();
        for op in &function.ops {
            if let Op::Call(target) = op {
                *seen.entry(*target).or_default() += 1;
            }
        }
        function.ops.retain(|op| !matches!(op, Op::Call(target) if seen[target] > self.budget));
    }
}

/// Not a violation of rule 4: the generator is given by the caller. `rand::Rng` is a trait
/// and is not one of the forbidden sources of randomness.
pub fn pick<R: Rng>(rng: &mut R, candidates: &[usize]) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    Some(candidates[rng.next_u32() as usize % candidates.len()])
}

/// Not a violation of rule 3: `fs`, `env`, `time`, `thread` and `process` are local names
/// here, not the modules of std.
pub fn local_names(env: &[usize], time: usize) -> usize {
    let fs = env.len();
    let thread = fs + time;
    let process = |net: usize| net + thread;
    process(fs)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test code may read files. Rule 3 leaves test code out with `except_in_test_code`.
    #[test]
    fn inlines_fixture() {
        let text = std::fs::read_to_string("fixture.ir").unwrap_or_default();
        let mut function = Function::new(&text);
        Inline { budget: 1 }.run(&mut function);
    }
}
