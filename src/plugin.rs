use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::System::LibraryLoader::{LoadLibraryW, GetProcAddress};

use crate::util::{DATA_DIR, has_extension, c, show_error, rel};

pub fn init() -> anyhow::Result<()> {
	for file in DATA_DIR.join("plugins").read_dir()? {
		let path = file?.path();
		if has_extension(&path, "dll") {
			show_error(c!(load_plugin(&path)?, "loading plugin {}", rel(&path).display()));
		}
	}
	Ok(())
}

fn load_plugin(path: &Path) -> anyhow::Result<()> {
	unsafe {
		let lib = LoadLibraryW(&HSTRING::from(path))?;
		if let Some(lb_init) = GetProcAddress(lib, windows::s!("lb_init")) {
			let lb_init: extern "C" fn() = std::mem::transmute(lb_init);
			lb_init();
		}
	};
	
	Ok(())
}
