//! The core of refpact as a WebAssembly module, for bindings in other languages.
//!
//! The module speaks the JSON requests of [`refpact_core::wire`] through four functions, and
//! needs no imports and no generated glue:
//!
//! 1. `refpact_alloc(len)` reserves `len` bytes and returns their address. The caller writes
//!    the request there, as UTF-8.
//! 2. `refpact_call(address, len)` answers the request and frees its bytes. It returns 0 for
//!    an answer and 1 for a request that is not valid.
//! 3. `refpact_answer()` and `refpact_answer_len()` give the address and the length of the
//!    text: the answer, or the reason that the request is not valid. The text stays until the
//!    next call.
//!
//! The module keeps one [`Session`], so a fact set that is loaded once serves many rules.
//! `xtask` builds it with `cargo xtask wasm`.

use std::cell::RefCell;

use refpact_core::wire::Session;

thread_local! {
    static SESSION: RefCell<Session> = RefCell::new(Session::new());
    static ANSWER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Reserves `len` bytes for a request. [`refpact_call`] frees them.
#[no_mangle]
pub extern "C" fn refpact_alloc(len: usize) -> *mut u8 {
    let bytes: Box<[u8]> = vec![0; len].into_boxed_slice();
    Box::into_raw(bytes).cast()
}

/// Answers the request of `len` bytes at `address` and frees the bytes. Returns 0 for an
/// answer and 1 for a request that is not valid.
///
/// # Safety
///
/// `address` and `len` must come from one call of [`refpact_alloc`], and the bytes must not
/// be used after this call.
#[no_mangle]
pub unsafe extern "C" fn refpact_call(address: *mut u8, len: usize) -> u32 {
    // SAFETY: the caller passes back the address and the length of `refpact_alloc`, which
    // made a boxed slice of that length.
    let bytes = unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(address, len)) };
    let (status, text) = match std::str::from_utf8(&bytes) {
        Ok(request) => match SESSION.with(|session| session.borrow_mut().call(request)) {
            Ok(answer) => (0, answer),
            Err(reason) => (1, reason),
        },
        Err(_) => (1, "the request is not UTF-8".to_owned()),
    };
    ANSWER.with(|answer| *answer.borrow_mut() = text.into_bytes());
    status
}

/// The address of the text of the last call.
#[no_mangle]
pub extern "C" fn refpact_answer() -> *const u8 {
    ANSWER.with(|answer| answer.borrow().as_ptr())
}

/// The length in bytes of the text of the last call.
#[no_mangle]
pub extern "C" fn refpact_answer_len() -> usize {
    ANSWER.with(|answer| answer.borrow().len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(request: &str) -> (u32, String) {
        let address = refpact_alloc(request.len());
        // SAFETY: the bytes were just reserved with this length.
        unsafe {
            std::ptr::copy_nonoverlapping(request.as_ptr(), address, request.len());
        }
        // SAFETY: the address and the length come from `refpact_alloc`.
        let status = unsafe { refpact_call(address, request.len()) };
        // SAFETY: the answer stays until the next call.
        let answer =
            unsafe { std::slice::from_raw_parts(refpact_answer(), refpact_answer_len()).to_vec() };
        (status, String::from_utf8(answer).unwrap())
    }

    #[test]
    fn requests_and_answers() {
        assert_eq!(
            call(r#"{"op": "version"}"#),
            (0, r#"{"version":1}"#.to_owned())
        );
        assert_eq!(
            call(r#"{"op": "fly"}"#),
            (1, "unknown request `fly`".to_owned())
        );
        assert_eq!(
            call(""),
            (1, "invalid JSON at byte 0: the text ends".to_owned())
        );
    }
}
