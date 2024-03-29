use camino::Utf8Path;
use eyre::Result;
use tracing::instrument;

use windows::core::HSTRING;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};

use crate::util::{catch, list_files, rel, DATA_DIR};

#[instrument]
pub fn init() -> Result<()> {
	let plugindir = DATA_DIR.join("plugins");
	if plugindir.exists() {
		for file in list_files(&plugindir, "dll")? {
			catch(load_plugin(&file));
		}
	} else {
		tracing::info!(dir = %rel(&plugindir), "plugin dir does not exist");
	}
	Ok(())
}

#[instrument(skip_all, fields(path = %rel(path)))]
fn load_plugin(path: &Utf8Path) -> Result<()> {
	unsafe {
		tracing::debug!("loading dll");
		let lib = LoadLibraryW(&HSTRING::from(path.as_str()))?;
		if let Some(lb_init) = GetProcAddress(lib, windows::core::s!("lb_init")) {
			let lb_init: extern "C" fn() = std::mem::transmute(lb_init);
			tracing::debug!("calling lb_init()");
			lb_init();
		}
	};

	Ok(())
}
