#![feature(decl_macro)]
#![feature(try_blocks)]

pub mod sigscan;
pub mod dir;
mod dirjson;
mod dllmain;
mod util;
mod plugin;

use std::path::{Path, PathBuf};

use eyre::{Result, bail};
use tracing::{instrument, field::display};

use windows::Win32::{
	Foundation::HANDLE,
	Storage::FileSystem::{
		GetFinalPathNameByHandleW,
		SetFilePointer,
		FILE_NAME,
		SET_FILE_POINTER_MOVE_METHOD,
	},
};

use sigscan::sigscan;
use dir::{DIRS, Entry};
use util::{DATA_DIR, EXE_PATH, rel, catch, windows_path, has_extension};

mod hooks {
	use retour::static_detour;
	static_detour! {
		pub static read_from_file: unsafe extern "thiscall" fn(*const super::HANDLE, *mut u8, usize) -> usize;
		pub static read_dir_files: unsafe extern "C" fn();
	}
}

#[instrument(skip_all)]
fn init() {
	tracing::info!(
		exe = %EXE_PATH.file_stem().unwrap().to_string_lossy(),
		data = %DATA_DIR.display(),
		"init",
	);
	catch(plugin::init());
	catch(init_lb_dir());
}

/// Initializes the hooks.
fn init_lb_dir() -> Result<()> {
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
/// This is called both for .dat and other files.
#[instrument(skip_all, fields(path, pos, len))]
fn read_from_file(handle: *const HANDLE, buf: *mut u8, len: usize) -> usize {
	// Get path to file
	let path = windows_path(|p| unsafe {
		GetFinalPathNameByHandleW(*handle, p, FILE_NAME(0))
	});

	// Get file offset
	let pos = unsafe {
		SetFilePointer(*handle, 0, None, SET_FILE_POINTER_MOVE_METHOD(1))
	} as usize;

	tracing::Span::current().record("path", &display(rel(&path)));
	tracing::Span::current().record("pos", pos);
	tracing::Span::current().record("len", len);

	// If the pathname refers to a .dat file, extract its number
	let dirnr = try {
		let name = path.file_name()?.to_str()?;
		let name = name.strip_prefix("ED6_DT")?.strip_suffix(".dat")?;
		usize::from_str_radix(name, 16).ok()?
	};

	if let Some(dirnr) = dirnr {
		// If it is a dir file, we still don't know *which* file is being loaded.
		// We have to check the dir file for a matching pos/len.
		let dirs = DIRS.lock().unwrap();
		let entry = dirs.entries()[dirnr].iter()
			.enumerate()
			.find(|(_, e)| e.offset == pos && e.csize == len);

		if let Some((filenr, entry)) = entry {
			let fileid = ((dirnr << 16) | filenr) as u32;
			if let Some(path) = get_redirect_file(fileid, entry) {
				tracing::debug!(path = %rel(&path), "redirecting");
				if let Some(v) = catch(read_file(&path)) {
					unsafe {
						std::ptr::copy_nonoverlapping(v.as_ptr(), buf, v.len());
					}
					return v.len()
				}
			}
		} else {
			tracing::warn!(pos, len, "no matching file");
		}
	}

	unsafe {
		hooks::read_from_file.call(handle, buf, len)
	}
}

/// Reads the file to be redirected to, if any.
#[instrument(skip_all, fields(fileid=?dirjson::Key::Id(fileid), entry = &*entry.name()))]
fn get_redirect_file(fileid: u32, entry: &Entry) -> Option<PathBuf> {
	let path = path_of(entry).map(|a| DATA_DIR.join(a));
	if let Some(path) = path {
		if path.exists() {
			tracing::debug!(path = %rel(&path), "explicit override");
			return Some(path)
		} else {
			tracing::error!(path = %rel(&path), "explicit override does not exist");
		}
	}

	let dirnr = fileid >> 16;
	let path = DATA_DIR.join(format!("ED6_DT{dirnr:02X}/{}", normalize_name(&entry.name())));
	tracing::debug!(path = %rel(&path), exists = path.exists(), "checking implicit override");
	path.exists().then_some(path)
}

/// Reads a file into memory, compressing it if necessary.
///
/// Most files in the dat files are compressed, but this is inconvenient for users so LB-ARK handles that implicitly.
///
/// Allocating memory here is not strictly necessary, but it makes the code much nicer.
#[instrument(skip_all, fields(path=%rel(path), is_raw))]
fn read_file(path: &Path) -> Result<Vec<u8>> {
	let ext: Option<_> = try { path.extension()?.to_str()?.to_lowercase() };
	let is_raw = ext.map_or(false, |e| e == "_ds" || e == "wav");
	tracing::Span::current().record("is_raw", is_raw);
	let data = std::fs::read(path)?;
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
/// This hook additionally loads `$DATA_DIR/*.dir`.
fn read_dir_files() {
	unsafe {
		hooks::read_dir_files.call();
	}

	catch(load_dir_files());
}

#[instrument(skip_all)]
fn load_dir_files() -> Result<()> {
	for file in DATA_DIR.read_dir()? {
		let path = file?.path();
		if has_extension(&path, "dir") {
			catch(parse_dir_file(&path));
		}
	}
	Ok(())
}

#[instrument(skip_all, fields(path = %rel(path)))]
fn parse_dir_file(path: &Path) -> Result<()> {
	let mut dirs = DIRS.lock().unwrap();
	let entries = serde_json::from_reader::<_, dirjson::DirJson>(std::fs::File::open(path)?)?;
	for (k, v) in entries.0 {
		catch(parse_dir_entry(&mut dirs, k, v));
	}
	Ok(())
}

#[instrument(skip_all, fields(key=?k, id))]
fn parse_dir_entry(dirs: &mut dir::Dirs, k: dirjson::Key, v: dirjson::Entry) -> Result<()> {
	let id = match k {
		dirjson::Key::Id(id) => id,
		dirjson::Key::Name(name) => match unnormalize_name(&name).and_then(|a| lookup_file(a, dirs)) {
			Some(id) => {
				tracing::Span::current().record("id", &display(format_args!("0x{id:08X}")));
				id
			},
			None => bail!("attempted to override file that doesn't exist"),
		},
	};

	let arc = id >> 16;
	let file = id as u16;
	if arc >= 64 {
		bail!("invalid file id: archive > 0x3F");
	}
	let entry = dirs.get(arc as u8, file);
	if let Some(prev) = path_of(entry) {
		bail!("file id already used by {}", prev);
	}

	let path = Box::leak(v.path.into_boxed_str());

	let name = v.name.as_deref()
		.or_else(|| Path::new(path).file_name().and_then(|a| a.to_str()))
		.and_then(unnormalize_name)
		.unwrap_or(*b"/_______.___");

	tracing::info!(name = %String::from_utf8_lossy(&name), path = %rel(Path::new(path)), "inserting override");

	*entry = Entry {
		name, // name
		offset: 0, // dat file is seeked to this position, so needs to be valid
		csize: id as usize | 0x80000000, // something unique, since the offsets are not
		unk1: path.as_ptr() as u32,
		unk2: path.len() as u32,
		asize: 888888888, // magic value to denote LB-ARK file
		ts: 0,
	};
	Ok(())
}

fn lookup_file(name: [u8; 12], dirs: &dir::Dirs) -> Option<u32> {
	for (i, arc) in dirs.entries().iter().enumerate() {
		for (j, e) in arc.iter().enumerate() {
			if e.name == name {
				return Some(((i << 16) | j) as u32)
			}
		}
	}
	None
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
