use crate::codegen; //~ use crate::codegen

pub fn call() -> u8 {
    codegen::generate() //~ expr crate::codegen::generate
}
