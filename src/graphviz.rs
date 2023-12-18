use std::ffi::{c_char, c_int, c_uint, CString};

use anyhow::{ensure, Context, Result};

pub fn render(dot_str: &str, layout: &str, format: &str) -> Result<Vec<u8>> {
    use graphviz_sys::*;

    let dot_str = CString::new(dot_str).context("Failed to convert dot_str to cstring")?;
    let layout = CString::new(layout).context("Failed to convert layout to cstring")?;
    let format = CString::new(format).context("Failed to convert format to cstring")?;

    unsafe {
        let gvc = gvContext();

        ensure!(!gvc.is_null(), "Failed to create context");

        let graph = agmemread(dot_str.as_ptr());

        ensure!(!graph.is_null(), "Failed to parse");

        gvLayout(gvc, graph, layout.as_ptr()).to_res("Failed to layout")?;

        let mut buffer_ptr: *mut c_char = std::ptr::null_mut();
        let mut data_size: c_uint = 0;
        gvRenderData(gvc, graph, format.as_ptr(), &mut buffer_ptr, &mut data_size)
            .to_res("Failed to render data")?;

        gvFreeLayout(gvc, graph).to_res("Failed to free layout")?;
        agclose(graph).to_res("Failed to close graph")?;
        gvFreeContext(gvc).to_res("Failed to free context")?;

        Ok(Vec::from_raw_parts(
            buffer_ptr as *mut u8,
            data_size as usize,
            data_size as usize,
        ))
    }
}

trait ToResult {
    fn to_res(&self, message: &'static str) -> Result<()>;
}

impl ToResult for c_int {
    fn to_res(&self, message: &'static str) -> Result<()> {
        ensure!(*self == 0, message);

        Ok(())
    }
}
