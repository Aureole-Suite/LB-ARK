#![feature(abi_thiscall)]

use std::borrow::Cow;
use std::cell::Cell;
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
	static ref ADDRS: Addrs = Addrs::get();
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
	read_dir_files: extern "C" fn(),
	dir_entries: &[Option<&[Cell<DirEntry>; 4096]>; 64],
	dir_n_entries: &[usize; 64],
}

macro_rules! sig {
	(@unit ?) => { None };
	(@unit $a:literal) => { Some($a) };
	(@unit $($t:tt)*) => { compile_error!(stringify!($($t)*)) };
	($($a:tt)*) => {
		&[$(sig!(@unit $a)),*]
	}
}

fn scan(sig: &[Option<u8>]) -> *const u8 {
	let start = 0x00400000;
	let data: &'static [u8] = unsafe {
		std::slice::from_raw_parts(start as *const u8, 0x00200000)
	};

	let Some(a) = sig[0] else { panic!() };
	let offset = memchr::memchr_iter(a, data)
		.find(|&a| data[a..].iter().zip(sig).all(|(a,b)| b.map_or(true, |b| *a==b)))
		.unwrap();

	(start + offset) as *const u8
}

impl Addrs {
	fn get() -> Addrs {
		let read_from_file = scan(sig! {
			0xA1 ? ? ? ?   // mov eax, ?
			0x83 0xEC 0x08 // sub esp, 8
			0xA3 ? ? ? ?   // mov ?, eax
		});

		let read_dir_files = scan(sig! {
			0x55                          // push ebp
			0x8B 0xEC                     // mov ebp, esp
			0x83 0xE4 0xF8                // and esp, ~7
			0x81 0xEC 0x9C 0x02 0x00 0x00 // sub esp, 0x29C
		});

		// at the end of read_dir_files, generally at +187
		let n = scan(sig! {
			0x89 0x34 0xBD ? ? ? ?  // mov dword ptr [edi*4 + dir_n_entries], esi
			0x81 0xC3 ? ? ? ?       // add ebx, ? ; 36*number of entries: 2047 in FC, 4096 in SC/3rd
			0x89 0x04 0xBD ? ? ? ?  // mov dword ptr [edi*4 + dir_entries], eax
			0x47                    // inc edi
		});
		let dir_n_entries = unsafe { *(n.add(3) as *const *const ()) };
		let dir_entries   = unsafe { *(n.add(16) as *const *const ()) };

		Addrs {
			read_from_file: read_from_file as usize,
			read_dir_files: read_dir_files as usize,
			dir_entries: dir_entries as usize,
			dir_n_entries: dir_n_entries as usize,
		}
	}
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct DirEntry {
	name: [u8; 12],
	unk1: u32,
	csize: usize,
	unk2: u32,
	asize: usize,
	ts: u32,
	offset: usize,
}

impl std::fmt::Debug for DirEntry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.name().fmt(f)
	}
}

impl DirEntry {
	fn name(&self) -> Cow<str> {
		if self.name == [0; 12] {
			"".into()
		} else {
			String::from_utf8_lossy(&self.name)
		}
	}
}

mod hooks {
	use retour::static_detour;
	static_detour! {
		pub static read_from_file: unsafe extern "thiscall" fn(*const super::HANDLE, *mut u8, usize) -> usize;
		pub static read_dir_files: unsafe extern "C" fn();
	}
}

fn init() -> anyhow::Result<()> {
	unsafe {
		hooks::read_from_file.initialize(ADDRS.read_from_file(), read_from_file)?.enable()?;
		hooks::read_dir_files.initialize(ADDRS.read_dir_files(), read_dir_files)?.enable()?;
	}

	Ok(())
}

fn read_dir_files() {
	unsafe {
		hooks::read_dir_files.call();
	}
	for (a, n) in ADDRS.dir_entries().iter().zip(ADDRS.dir_n_entries()) {
		println!("{n} {:?}", a.map(|a| a.iter().map(|a| a.get()).collect::<Vec<_>>()));
	}
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

		let entry = ADDRS.dir_entries()[nr].unwrap().iter()
			.map(|a| a.get())
			.enumerate()
			.find(|(_, e)| e.offset == pos && e.csize == len);

		if let Some((_, entry)) = entry {
			let data_dir = path.parent().unwrap().join("data");
			let dir = data_dir.join(format!("ED6_DT{nr:02X}"));

			for path in [
				dir.join(normalize_name(&entry.name())),
				dir.join(&*entry.name()),
			] {
				if path.exists() {
					if is_raw(&entry.name()) {
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

	unsafe {
		hooks::read_from_file.call(handle, buf, len)
	}
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
