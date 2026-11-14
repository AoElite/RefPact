pub struct Local;

impl crate::codegen::Backend for Local { //~ impl crate::codegen::Backend
    fn emit(&self) -> u8 {
        0
    }
}

pub fn call() -> u8 {
    <Local as crate::codegen::Backend>::emit(&Local) //~ expr crate::codegen::Backend
}
