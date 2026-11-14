pub fn generic<T: crate::codegen::Backend>(value: T) {} //~ bound crate::codegen::Backend

pub fn where_clause<T>(value: T)
where
    T: crate::codegen::Backend, //~ bound crate::codegen::Backend
{
}

pub fn impl_trait(value: impl crate::codegen::Backend) {} //~ bound crate::codegen::Backend

pub fn dyn_trait(value: &dyn crate::codegen::Backend) {} //~ bound crate::codegen::Backend

pub trait Sub: crate::codegen::Backend {} //~ bound crate::codegen::Backend

pub struct Holder<T: crate::codegen::Backend>(T); //~ bound crate::codegen::Backend
