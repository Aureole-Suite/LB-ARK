use retour::static_detour;

use windows::core::HRESULT;
use windows::Win32::Foundation::{BOOL, HMODULE, TRUE};
use windows::Win32::System::{
	LibraryLoader::{GetProcAddress, LoadLibraryA},
	ProcessStatus::{GetModuleInformation, MODULEINFO},
	SystemServices::DLL_PROCESS_ATTACH,
	Threading::{GetCurrentProcess, PEB},
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HMODULE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason != DLL_PROCESS_ATTACH {
		return TRUE;
	}

	init_tracing();

	tracing::debug!("LB-DIR inject init hook");

	unsafe {
		let mut modinfo = MODULEINFO::default();
		GetModuleInformation(
			GetCurrentProcess(),
			HMODULE(0),
			&mut modinfo,
			std::mem::size_of::<MODULEINFO>() as u32,
		)
		.unwrap();

		main_detour
			.initialize(std::mem::transmute(modinfo.EntryPoint), main_hook)
			.unwrap()
			.enable()
			.unwrap();
	}

	TRUE
}

fn init_tracing() {
	use tracing_error::ErrorLayer;
	use tracing_subscriber::prelude::*;
	use tracing_subscriber::{fmt, EnvFilter};

	let fmt_layer = fmt::layer().with_target(false);
	let filter_layer = EnvFilter::try_from_default_env()
		.or_else(|_| EnvFilter::try_new("info"))
		.unwrap();

	tracing_subscriber::registry()
		.with(filter_layer)
		.with(fmt_layer)
		.with(ErrorLayer::default())
		.init();

	eyre_span::install().unwrap();
}

static_detour! {
	pub static main_detour: extern "C" fn(*const PEB) -> u32;
}

fn main_hook(peb: *const PEB) -> u32 {
	super::init();
	main_detour.call(peb)
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
