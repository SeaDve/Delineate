#![allow(
    non_snake_case,
    non_upper_case_globals,
    non_camel_case_types,
    improper_ctypes // See https://github.com/rust-lang/rust/issues/54341
)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
