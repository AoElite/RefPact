use tracekit::traced; //~ use tracekit::traced

#[tracekit::traced] //~ attribute tracekit::traced
pub fn full_path() {}

#[traced] //~ attribute tracekit::traced
pub fn imported() {}

#[derive(Debug, tracekit::Marker)] //~ attribute tracekit::Marker
pub struct Derived;

#[cfg_attr(all(), derive(tracekit::Marker))] //~ attribute tracekit::Marker
pub struct Conditional;

#[derive(tracekit::Marker)] //~ attribute tracekit::Marker
#[trace(with = crate::codegen::generate)] //~ attribute crate::codegen::generate
pub struct WithArgument;
