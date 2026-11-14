pub struct Holder {
    pub generator: crate::codegen::Generator, //~ type crate::codegen::Generator
}

pub type Alias = crate::codegen::Generator; //~ type crate::codegen::Generator

pub fn take(generator: &crate::codegen::Generator) {} //~ type crate::codegen::Generator

pub fn give() -> Option<crate::codegen::Config> { //~ type crate::codegen::Config
    None
}

pub fn locals() -> usize {
    let list: Vec<crate::codegen::Mode> = Vec::new(); //~ type crate::codegen::Mode
    let cast = 1u8 as crate::codegen::Byte; //~ type crate::codegen::Byte
    std::mem::size_of::<crate::codegen::Config>() //~ type crate::codegen::Config
}
