use crate::codegen::MAX; //~ use crate::codegen::MAX

pub fn classify(
    mode: crate::codegen::Mode, //~ type crate::codegen::Mode
    config: crate::codegen::Config, //~ type crate::codegen::Config
    wrapper: crate::codegen::Wrapper, //~ type crate::codegen::Wrapper
    size: usize,
) -> u8 {
    let a = match mode {
        crate::codegen::Mode::Fast => 1, //~ pattern crate::codegen::Mode::Fast
        _ => 2,
    };
    let crate::codegen::Config { size: b } = config; //~ pattern crate::codegen::Config
    let crate::codegen::Wrapper(c) = wrapper; //~ pattern crate::codegen::Wrapper
    let d = match size {
        MAX => 1, //~ pattern crate::codegen::MAX
        0..=crate::codegen::SMALL => 2, //~ pattern crate::codegen::SMALL
        max => 3,
    };
    a + b as u8 + c + d
}
