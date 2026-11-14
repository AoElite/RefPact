mod local {
    pub use crate::codegen::Generator; //~ use crate::codegen::Generator
}

use self::local::Generator; //~ use crate::codegen::Generator written=crate::forms::use_self::local::Generator
