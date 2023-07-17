use windows::core::HRESULT;
use windows::Win32::{
	Foundation::{BOOL, HMODULE, TRUE},
	System::LibraryLoader::{
		LoadLibraryA,
		GetProcAddress,
	},
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HMODULE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason == 1 /* DLL_PROCESS_ATTACH */ {
		crate::init();
	}
	TRUE
}

#[no_mangle]
#[allow(non_snake_case, non_upper_case_globals)]
pub extern "system" fn DirectXFileCreate(dxfile: *const *const ()) -> HRESULT {
	lazy_static::lazy_static! {
		static ref next_DirectXFileCreate: extern "system" fn(*const *const ()) -> HRESULT = unsafe {
			let lib = LoadLibraryA(windows::s!("C:\\Windows\\System32\\d3dxof.dll")).unwrap();
			let w = GetProcAddress(lib, windows::s!("DirectXFileCreate")).unwrap();
			std::mem::transmute(w)
		};
	}
	next_DirectXFileCreate(dxfile)
}
