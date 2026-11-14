//! A trait of the storage, implemented for a type of the domain through its re-export.

pub trait Stored {}

impl Stored for shop_domain::Order {}
