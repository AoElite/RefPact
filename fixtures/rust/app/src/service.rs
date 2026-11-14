//! The entry point that ties the layers together.

use crate::cache::Cache;
use crate::execution::execute;
use crate::ir::Function;
use crate::platform::clock::SystemClock;
use crate::platform::entropy;
use crate::platform::fs::Disk;

pub fn serve(function: &Function) -> i64 {
    let mut cache = Cache::new(Disk);
    cache.put(&function.name, &entropy::seed().to_le_bytes());
    execute(function, SystemClock, false)
}
