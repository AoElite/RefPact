pub struct Local;

impl crate::codegen::Backend for Local { //~ impl crate::codegen::Backend
    fn emit(&self) -> u8 {
        0
    }
}
