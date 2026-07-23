mod binding;

use pyo3::prelude::*;

#[pyfunction]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[pyfunction]
fn default_debugger_address() -> String {
    openpage::LaunchOptions::default().address()
}

#[pymodule]
fn openpage_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(default_debugger_address, m)?)?;
    binding::register(m)?;
    Ok(())
}
