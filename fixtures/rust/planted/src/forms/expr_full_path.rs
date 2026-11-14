pub fn build() -> usize {
    let generator = crate::codegen::Generator::new(); //~ expr crate::codegen::Generator::new
    let text = std::fs::read_to_string("x"); //~ expr std::fs::read_to_string
    let doubled = [1u8].map(crate::codegen::double); //~ expr crate::codegen::double
    let config = crate::codegen::Config { size: 1 }; //~ expr crate::codegen::Config
    crate::codegen::MAX //~ expr crate::codegen::MAX
}
