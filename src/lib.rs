//! blazeweb — URL in, fully-rendered HTML (and/or screenshot) out.
//!
//! This crate is the Rust side of the `blazeweb` Python package. The Python
//! side (`python/blazeweb/__init__.py`) is the user-facing API; this module
//! is the compiled extension `blazeweb._blazeweb`.

use pyo3::prelude::*;

mod chrome;
mod client;
mod config;
mod dom;
mod engine;
mod error;
mod result;
mod runtime;

use client::Client;
use dom::{Dom, Element};
use result::{RawFetchOutput, RawRenderOutput};

#[pymodule]
#[pyo3(name = "_blazeweb")]
fn blazeweb_module(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let _ = env_logger::try_init();

    // Eager-init the tokio runtime so first Client()/fetch() call doesn't pay it.
    let _ = runtime::shared();

    m.add_class::<Client>()?;
    m.add_class::<RawRenderOutput>()?;
    m.add_class::<RawFetchOutput>()?;
    m.add_class::<Dom>()?;
    m.add_class::<Element>()?;
    Ok(())
}
