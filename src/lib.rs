#![feature(abi_thiscall)]
#![feature(once_cell)]
#![feature(decl_macro)]
#![feature(try_blocks)]

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::os::windows::prelude::OsStrExt;
use std::path::{PathBuf, Path};

use windows::core::{HRESULT, PCWSTR};
use windows::Win32::{
	Foundation::{BOOL, HANDLE, HMODULE, TRUE},
	Storage::FileSystem::{
		GetFinalPathNameByHandleW,
		SetFilePointer,
		FILE_NAME,
		SET_FILE_POINTER_MOVE_METHOD,
	},
	System::LibraryLoader::GetModuleFileNameW,
	UI::WindowsAndMessaging::{MessageBoxW, MESSAGEBOX_STYLE},
};

pub mod sigscan;
pub mod dir;

use sigscan::sigscan;
use dir::{DIRS, Entry};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HMODULE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason != 1 /* DLL_PROCESS_ATTACH */ { return TRUE }

	println!("LB-ARK: init for {}", EXE_PATH.file_stem().unwrap().to_string_lossy());

	show_error(init()).is_some().into()
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DirectXFileCreate(_dxfile: *const *const ()) -> HRESULT {
	// I don't think this function is ever called. If I'm wrong, oh well.
	show_error::<()>(Err(anyhow::anyhow!("DirectXFileCreate called")));
	std::process::abort()
}

lazy_static::lazy_static! {
	/// Path to the main executable, generally named `ed6_win_something.exe`.
	static ref EXE_PATH: PathBuf = {
		let mut path = [0; 260];
		let n = unsafe {
			GetModuleFileNameW(HMODULE(0), &mut path)
		};
		let path = OsString::from_wide(&path[..n as usize]);
		PathBuf::from(path)
	};
	/// Path to the game directory, where all game files are located.
	static ref GAME_DIR: &'static Path = EXE_PATH.parent().unwrap();
	/// Path to LB-DIR data directory, where LB-DIR reads the files from.
	static ref DATA_DIR: PathBuf = GAME_DIR.join("data");
}

mod hooks {
	use retour::static_detour;
	static_detour! {
		pub static read_from_file: unsafe extern "thiscall" fn(*const super::HANDLE, *mut u8, usize) -> usize;
		pub static read_dir_files: unsafe extern "C" fn();
	}
}

/// Initializes the hooks.
fn init() -> anyhow::Result<()> {
	unsafe {
		hooks::read_from_file.initialize(std::mem::transmute(sigscan! {
			0xA1 ? ? ? ?   // mov eax, ?
			0x83 0xEC 0x08 // sub esp, 8
			0xA3 ? ? ? ?   // mov ?, eax
		}), read_from_file)?.enable()?;

		hooks::read_dir_files.initialize(std::mem::transmute(sigscan! {
			0x55                          // push ebp
			0x8B 0xEC                     // mov ebp, esp
			0x83 0xE4 0xF8                // and esp, ~7
			0x81 0xEC 0x9C 0x02 0x00 0x00 // sub esp, 0x29C
		}), read_dir_files)?.enable()?;
	}

	Ok(())
}

/// Called by the game to read from any file into memory.
///
/// This is called both for .dat and other files
fn read_from_file(handle: *const HANDLE, buf: *mut u8, len: usize) -> usize {
	// Get path to file
	let mut path = [0; 260];
	let n = unsafe {
		GetFinalPathNameByHandleW(*handle, &mut path, FILE_NAME(0))
	} as usize;
	let path = PathBuf::from(OsString::from_wide(&path[..n]));

	// If the pathname refers to a .dat file, extract its number
	let dirnr = try {
		let name = path.file_name()?.to_str()?;
		let name = name.strip_prefix("ED6_DT")?.strip_suffix(".dat")?;
		usize::from_str_radix(name, 16).ok()?
	};

	if let Some(dirnr) = dirnr {
		// If it is a dir file, we still don't know *which* file is being loaded.
		// All we have is the file position, which is set by a different function.
		// So we extract the offset and find the file with the corresponding offset from the .dat file.
		let pos = unsafe {
			SetFilePointer(*handle, 0, None, SET_FILE_POINTER_MOVE_METHOD(1))
		} as usize;

		let dirs = DIRS.lock().unwrap();
		let entry = dirs.entries()[dirnr].iter()
			.enumerate()
			.find(|(_, e)| e.offset == pos && e.csize == len);

		if let Some((filenr, entry)) = entry {
			let fileid = ((dirnr << 16) | filenr) as u32;
			// We only have an unstructured buffer to write to.
			if let Some(v) = get_redirect_file(fileid, entry) {
				let buf = unsafe { std::slice::from_raw_parts_mut(buf, v.len()) };
				buf.copy_from_slice(&v);
				return v.len()
			}
		}
	}

	unsafe {
		hooks::read_from_file.call(handle, buf, len)
	}
}

/// Reads the file to be redirected to, if any.
///
/// Allocating memory here is not strictly necessary, but it makes the code much nicer.
fn get_redirect_file(fileid: u32, entry: &Entry) -> Option<Vec<u8>> {
	let dirnr = fileid >> 16;
	let path = if let Some(path) = path_of(entry) {
		Some(DATA_DIR.join(format!("ED6_DT{dirnr:02X}")).join(path))
	} else {
		Some(DATA_DIR.join(format!("ED6_DT{dirnr:02X}/{}", normalize_name(&entry.name()))))
			.filter(|a| a.exists())
	};

	if let Some(path) = path {
		show_error(c!(read_file(&path)?, "failed to read {}", rel(&path).display()))
	} else {
		None
	}
}

/// Reads a file into memory, compressing it if necessary.
///
/// Most files in the dat files are compressed, but this is inconvenient for users so LB-ARK handles that implicitly.
fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
	let data = std::fs::read(path)?;
	let ext: Option<_> = try { path.extension()?.to_str()?.to_lowercase() };
	let is_raw = ext.map_or(false, |e| e == "_ds" || e == "wav");
	if is_raw {
		Ok(data)
	} else {
		Ok(fake_compress(&data))
	}
}

/// "Compress" data by inserting "raw data" instructions as necessary.
fn fake_compress(data: &[u8]) -> Vec<u8> {
	let mut chunks = data.chunks(0x1FFF).peekable();
	let mut out = Vec::with_capacity(3 + data.len() + chunks.clone().len() * 5);

	// include an empty chunk, because otherwise it'll just read uninitialized data
	out.extend(&u16::to_le_bytes(2)); // chunk size
	out.push(chunks.peek().is_some().into()); // has next chunk?

	while let Some(chunk) = chunks.next() {
		let len = chunk.len() as u16;
		out.extend(&u16::to_le_bytes(len + 4)); // chunk size
		out.extend(&u16::to_be_bytes(len | 0x2000)); // compression flags: N bits of raw data
		out.extend(chunk); // data
		out.push(chunks.peek().is_some().into()); // has next chunk?
	}

	out
}


/// Called by the game at startup, to load the .dir files into memory.
///
/// This hook additionally loads `$DATA_DIR/ED6_DT??/*.dir`.
fn read_dir_files() {
	unsafe {
		hooks::read_dir_files.call();
	}

	show_error(c!(do_load_dir(), "failed to load dir files"));
}

fn do_load_dir() -> anyhow::Result<()> {
	for nr in 0..64 {
		let dir = DATA_DIR.join(format!("ED6_DT{nr:02X}"));
		if dir.is_dir() {
			for f in dir.read_dir()? {
				let path = f?.path();
				let ext = path.extension().and_then(|a| a.to_str());
				if ext.map_or(true, |a| a.to_lowercase() != "dir") {
					continue
				}

				for (n, line) in std::fs::read_to_string(&path)?.lines().enumerate() {
					show_error(c!({
						parse_dir_line(nr, line)?
					}, "{}, line {}", rel(&path).display(), n+1));
				}
			}
		}
	}
	Ok(())
}

fn parse_dir_line(arc: usize, line: &str) -> anyhow::Result<()> {
	let (line, _) = line.split_once('#').unwrap_or((line, ""));
	let line = line.trim();
	if line.is_empty() {
		return Ok(())
	}

	let Some((n, name)) = line.split_once(' ')
		.or_else(|| line.split_once('\t'))
		else {
			anyhow::bail!("no space in line")
		};

	let n = if let Some(s) = n.strip_prefix("0x") {
		u16::from_str_radix(s, 16)?
	} else {
		n.parse::<u16>()?
	};

	let name = name.trim();

	let mut dirs = DIRS.lock().unwrap();
	let entry = dirs.get(arc as u8, n);
	if entry.name != Entry::default().name {
		let prev = path_of(entry).map_or_else(|| entry.name(), |n| n.into());
		anyhow::bail!("index {n} is already used by {}", prev);
	}

	entry.name = unnormalize_name(name).unwrap_or(*b"98_invalid__");
	entry.offset = 0;
	entry.csize = n as usize;
	entry.asize = 888888888;
	entry.unk1 = Box::leak(name.to_owned().into_boxed_str()).as_ptr() as usize as u32;
	entry.unk2 = name.len() as u32;

	Ok(())
}

fn path_of(e: &Entry) -> Option<&str> {
	if e.offset == 0 && e.asize == 888888888 {
		Some(unsafe {
			let ptr = e.unk1 as usize as *const u8;
			let slice = std::slice::from_raw_parts(ptr, e.unk2 as usize);
			std::str::from_utf8_unchecked(slice)
		})
	} else {
		None
	}
}

pub fn normalize_name(name: &str) -> String {
	let name = name.to_lowercase();
	if let Some((name, ext)) = name.split_once('.') {
		format!("{}.{ext}", name.trim_end_matches(' '))
	} else {
		name
	}
}

pub fn unnormalize_name(name: &str) -> Option<[u8; 12]> {
	let (_, name) = name.rsplit_once(['/', '\\']).unwrap_or(("", name));
	let name = name.to_uppercase();
	let (name, ext) = name.split_once('.').unwrap_or((&name, ""));
	if name.len() > 8 || ext.len() > 3 { return None; }
	let mut o = *b"        .   ";
	o[..name.len()].copy_from_slice(name.as_bytes());
	o[9..][..ext.len()].copy_from_slice(ext.as_bytes());
	Some(o)
}

fn msgbox(title: &str, body: &str, style: u32) -> u32 {
	let mut title = OsString::from(title).encode_wide().collect::<Vec<_>>();
	let mut body = OsString::from(body).encode_wide().collect::<Vec<_>>();
	title.push(0);
	body.push(0);
	unsafe {
		MessageBoxW(
			None,
			PCWSTR::from_raw(body.as_ptr()),
			PCWSTR::from_raw(title.as_ptr()),
			MESSAGEBOX_STYLE(style)
		).0 as u32
	}
}

/// A nicer syntax for [`with_context`].
macro c($e:expr, $($a:tt)*) {
	<anyhow::Result<_> as anyhow::Context<_, _>>::with_context(try { $e }, || format!($($a)*))
}

/// Shows an error in an appropriate way, returning the value as an `Option`.
fn show_error<T>(a: anyhow::Result<T>) -> Option<T> {
	match a {
		Ok(v) => Some(v),
		Err(e) => {
			let s = format!("{e:#}");
			println!("{:?}", e.context("LB-ARK error"));
			msgbox("LB-ARK error", &s, 0x10);
			None
		}
	}
}

/// Converts the path to be relative to the game directory, for nicer error messages.
fn rel(path: &Path) -> &Path {
	path.strip_prefix(*GAME_DIR).unwrap_or(path)
}
