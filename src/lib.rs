#![feature(abi_thiscall)]

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStringExt;
use std::path::{PathBuf, Path};

use windows::core::HRESULT;
use windows::Win32::{
	Foundation::{BOOL, FALSE, HANDLE, HINSTANCE, TRUE},
	Storage::FileSystem::{
		GetFinalPathNameByHandleW,
		SetFilePointer,
		FILE_NAME,
		SET_FILE_POINTER_MOVE_METHOD,
	},
	System::LibraryLoader::GetModuleFileNameW
};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HINSTANCE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason != 1 /* DLL_PROCESS_ATTACH */ { return TRUE }

	println!("LB-ARK: init for {}", *NAME);

	match init() {
		Ok(()) => TRUE,
		Err(e) => {
			println!("LB-ARK: init failed: {e:?}");
			FALSE
		}
	}
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DirectXFileCreate(_dxfile: *const *const ()) -> HRESULT {
	// I don't think this function is ever called. If I'm wrong, oh well.
	println!("LB-ARK: DirectXFileCreate called");
	std::process::abort()
}

lazy_static::lazy_static! {
	static ref NAME: &'static str = {
		let mut path = [0; 260];
		let n = unsafe {
			GetModuleFileNameW(HINSTANCE(0), &mut path)
		};
		let path = OsString::from_wide(&path[..n as usize]);
		let name = Path::new(&path).file_stem().unwrap().to_str().unwrap();
		Box::leak(name.to_lowercase().into_boxed_str())
	};
	static ref ADDRS: Addrs = Addrs::get(*NAME).expect("exe not supported");
}

macro_rules! Addrs {
	($($k:ident: $ty:ty),* $(,)?) => {
		#[derive(Debug, Clone, Copy, PartialEq, Eq)]
		struct Addrs {
			$($k: usize,)*
		}

		impl Addrs {
			$(fn $k(&self) -> $ty { unsafe { std::mem::transmute(self.$k) } })*
		}
	}
}

Addrs! {
	read_from_file: extern "thiscall" fn(*const HANDLE, *mut u8, usize) -> usize,
	dir_entries: &[&[DirEntry; 4096]; 64],
}

impl Addrs {
	fn get(name: &str) -> Option<Addrs> {
		match name {
			"ed6_win3_dx9" => Some(Addrs {
				read_from_file: 0x004A4DD0,
				dir_entries:    0x00992DC0,
			}),
			_ => None,
		}
	}
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DirEntry {
	name: [u8; 12],
	unk1: u32,
	csize: usize,
	unk2: u32,
	asize: usize,
	ts: u32,
	offset: usize,
}

impl DirEntry {
	fn name(&self) -> Cow<str> {
		String::from_utf8_lossy(&self.name)
	}
}

mod hooks {
	use retour::static_detour;
	static_detour! {
		pub static read_from_file: extern "thiscall" fn(*const super::HANDLE, *mut u8, usize) -> usize;
	}
}

fn init() -> anyhow::Result<()> {
	unsafe {
		hooks::read_from_file.initialize(ADDRS.read_from_file(), read_from_file)?.enable()?;
	}

	Ok(())
}

fn read_from_file(handle: *const HANDLE, buf: *mut u8, len: usize) -> usize {
	let mut path = [0; 260];
	let n = unsafe {
		GetFinalPathNameByHandleW(*handle, &mut path, FILE_NAME(0))
	} as usize;
	let path = OsString::from_wide(&path[..n]);
	let path = PathBuf::from(path);

	if let Some(nr) = parse_archive_nr(&path) {
		let pos = unsafe {
			SetFilePointer(*handle, 0, None, SET_FILE_POINTER_MOVE_METHOD(1))
		} as usize;

		let index = ADDRS.dir_entries()[nr].iter()
			.position(|e| e.offset == pos && e.csize == len);

		if let Some(index) = index {
			let raw_name = ADDRS.dir_entries()[nr][index].name();
			let data_dir = path.parent().unwrap().join("data");
			let dir = data_dir.join(format!("ED6_DT{nr:02X}"));

			for path in [
				dir.join(normalize_name(&raw_name)),
				dir.join(&*raw_name),
			] {
				if path.exists() {
					if is_raw(&raw_name) {
						let mut f = std::fs::File::open(path).unwrap();
						let buf = unsafe { std::slice::from_raw_parts_mut(buf, len) };
						f.read_exact(buf).unwrap();
						return len
					} else {
						let data = std::fs::read(path).unwrap();
						let buf = unsafe { std::slice::from_raw_parts_mut(buf, 0x600000) };
						let mut f = std::io::Cursor::new(buf);
						fake_compress(&mut f, &data).unwrap();
						return f.position() as usize
					}
				}
			}
		}
	}

	hooks::read_from_file.call(handle, buf, len)
}

fn parse_archive_nr(path: &Path) -> Option<usize> {
	let name = path.file_name()?.to_str()?;
	let name = name.strip_prefix("ED6_DT")?.strip_suffix(".dat")?;
	usize::from_str_radix(name, 16).ok()
}

fn fake_compress(buf: &mut impl Write, data: &[u8]) -> anyhow::Result<()> {
	let mut chunks = data.chunks(0x1FFF).peekable();
	// include an empty chunk, because otherwise it'll just read uninitialized data
	buf.write_all(&u16::to_le_bytes(2))?;
	buf.write_all(&[chunks.peek().is_some().into()])?;
	while let Some(chunk) = chunks.next() {
		let len = chunk.len() as u16;
		buf.write_all(&u16::to_le_bytes(len + 4))?;
		buf.write_all(&u16::to_be_bytes(len | 0x2000))?; 
		buf.write_all(chunk)?;
		buf.write_all(&[chunks.peek().is_some().into()])?;
	}
	Ok(())
}

pub fn normalize_name(name: &str) -> String {
	let name = name.to_lowercase();
	if let Some((name, ext)) = name.split_once('.') {
		format!("{}.{ext}", name.trim_end_matches(' '))
	} else {
		name
	}
}

pub fn is_raw(name: &str) -> bool {
	name.ends_with("._DS") || name.ends_with(".WAV")
}
