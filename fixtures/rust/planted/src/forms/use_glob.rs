use crate::codegen::*; //~ use-glob crate::codegen

pub fn build() -> Generator { //~ type crate::codegen::Generator
    Generator::new() //~ expr crate::codegen::Generator::new
}
