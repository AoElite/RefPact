use crate::facade::os; //~ use std::fs written=crate::facade::os

pub fn build() -> bool {
    let a = crate::facade::Generator::new(); //~ expr crate::codegen::Generator::new written=crate::facade::Generator::new
    let b = crate::facade::deep::Config { size: 1 }; //~ expr crate::codegen::Config written=crate::facade::deep::Config
    os::read("x").is_ok() //~ expr std::fs::read written=crate::facade::os::read
}
