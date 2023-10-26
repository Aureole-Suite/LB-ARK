use retour::static_detour;

use windows::core::HRESULT;
use windows::Win32::{
	Foundation::{BOOL, HMODULE, TRUE},
	System::{
		Diagnostics::Debug::IMAGE_NT_HEADERS32,
		LibraryLoader::{GetProcAddress, LoadLibraryA},
		SystemServices::IMAGE_DOS_HEADER,
		Threading::PEB,
	},
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HMODULE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason != 1 /* DLL_PROCESS_ATTACH */ { return TRUE }

	init_tracing();

	tracing::debug!("LB-DIR inject init hook");

	unsafe {
		let peb: *const PEB;
		std::arch::asm!("mov {0}, fs:[0x30]", out(reg) peb);
		let base = (*peb).Reserved3[1] as *const u8; // Officially a HMODULE, but it's a pointer
		let head_dos = base as *const IMAGE_DOS_HEADER;
		let head_nt = base.offset((*head_dos).e_lfanew as isize) as *const IMAGE_NT_HEADERS32;
		let entry = base.add((*head_nt).OptionalHeader.AddressOfEntryPoint as usize);
		main_detour.initialize(std::mem::transmute(entry), main_hook).unwrap().enable().unwrap();
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
