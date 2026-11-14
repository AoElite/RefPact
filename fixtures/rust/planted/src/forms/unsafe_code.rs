pub unsafe fn raw() {} //~ marker unsafe-fn

pub fn block() -> u8 {
    let bytes = [1u8];
    unsafe { *bytes.get_unchecked(0) } //~ marker unsafe-block
}

pub unsafe trait Trusted {} //~ marker unsafe-trait

unsafe impl Trusted for u8 {} //~ marker unsafe-impl

unsafe extern "C" { //~ marker unsafe-extern
    pub fn abs(value: i32) -> i32;
}

#[unsafe(no_mangle)] //~ marker unsafe-attr
pub extern "C" fn refpact_fixture_exported() {}

pub struct Holder;

impl Holder {
    pub unsafe fn method(&self) {} //~ marker unsafe-fn
}

pub trait Raw {
    unsafe fn required(&self); //~ marker unsafe-fn
}
