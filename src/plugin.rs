use std::path::Path;

use color_eyre::eyre::Result;
use tracing::instrument;

use windows::core::HSTRING;
use windows::Win32::System::LibraryLoader::{LoadLibraryW, GetProcAddress};

use crate::util::{DATA_DIR, has_extension, catch, rel};

#[instrument]
pub fn init() -> Result<()> {
	for file in DATA_DIR.join("plugins").read_dir()? {
		let path = file?.path();
		if has_extension(&path, "dll") {
			catch(load_plugin(&path));
		}
	}
	Ok(())
}

#[instrument(skip_all, fields(path = %rel(path)))]
fn load_plugin(path: &Path) -> Result<()> {
	unsafe {
		tracing::debug!("loading dll");
		let lib = LoadLibraryW(&HSTRING::from(path))?;
		if let Some(lb_init) = GetProcAddress(lib, windows::s!("lb_init")) {
			let lb_init: extern "C" fn() = std::mem::transmute(lb_init);
			tracing::debug!("calling lb_init()");
			lb_init();
		}
	};
	
	Ok(())
}
