//! Test code is checked like other code unless the rule says `except_in_test_code`.

pub fn double(value: u8) -> u8 {
    value * 2
}

#[cfg(test)]
mod tests {
    use std::fs; //~ use std::fs

    #[test]
    fn reads() {
        assert!(fs::read("x").is_err()); //~ expr std::fs::read
    }
}

#[test]
fn top_level() {
    assert!(std::env::var("REFPACT_NO_SUCH_VARIABLE").is_err()); //~ expr std::env::var
}
