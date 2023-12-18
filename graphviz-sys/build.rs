use bindgen::callbacks::{MacroParsingBehavior, ParseCallbacks};

use std::collections::HashSet;
use std::env;
use std::path::PathBuf;

#[derive(Debug)]
struct IgnoreMacros<'a>(HashSet<&'a str>);

impl ParseCallbacks for IgnoreMacros<'_> {
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        if self.0.contains(name) {
            MacroParsingBehavior::Ignore
        } else {
            MacroParsingBehavior::Default
        }
    }
}

fn main() {
    let libs = system_deps::Config::new().probe().unwrap();

    let headers = libs.all_include_paths();

    let mut builder = bindgen::builder()
        .header("wrapper.h")
        .parse_callbacks(Box::new(IgnoreMacros(HashSet::from_iter([
            "FP_INFINITE",
            "FP_NAN",
            "FP_NORMAL",
            "FP_SUBNORMAL",
            "FP_ZERO",
        ]))));

    for header in headers {
        builder = builder.clang_arg("-I").clang_arg(header.to_str().unwrap());
    }

    let bindings = builder.generate().unwrap();

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .unwrap();
}
