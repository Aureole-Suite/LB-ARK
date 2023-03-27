#![feature(naked_functions)]

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::{Read, Write, Seek, SeekFrom};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
use windows::core::HRESULT;
use windows::Win32::Foundation::{HINSTANCE, BOOL, TRUE, FALSE};

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DllMain(_dll_module: HINSTANCE, reason: u32, _reserved: *const ()) -> BOOL {
	if reason != 1 /* DLL_PROCESS_ATTACH */ { return TRUE }

	// println!("SoraData: init for {}", *NAME);

	match init() {
		Ok(()) => TRUE,
		Err(e) => {
			println!("SoraData: init failed: {e:?}");
			FALSE
		}
	}
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "system" fn DirectXFileCreate(_dxfile: *const *const ()) -> HRESULT {
	// I don't think this function is ever called. If I'm wrong, oh well.
	println!("SoraData: DirectXFileCreate called");
	std::process::abort()
}

lazy_static::lazy_static! {
	static ref PATH: PathBuf = {
		let mut path = [0; 260];
		unsafe {
			GetModuleFileNameW(HINSTANCE(0), &mut path);
		}
		let path = &path[..path.iter().position(|a| *a == 0).expect("has nul")];
		PathBuf::from(OsString::from_wide(path))
	};
	static ref NAME: &'static str = {
		let path = PATH.to_str().expect("is utf8");
		let name = path.rsplit('\\').next().expect("split is nonempty");
		let name = Box::leak(name.to_lowercase().into_boxed_str());
		name.strip_suffix(".exe").expect("ends with .exe")
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
	read_from_dat: unsafe extern "fastcall" fn(*mut u8, u32, usize, usize) -> bool,
	dir_entries: &[&[DirEntry; 4096]; 64],
}

impl Addrs {
	fn get(name: &str) -> Option<Addrs> {
		match name {
			"ed6_win3_dx9" => Some(Addrs {
				read_from_dat: 0x004A2C50,
				dir_entries:   0x00992DC0,
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

fn init() -> anyhow::Result<()> {
	unsafe {
		tour(ADDRS.read_from_dat() as *const (), read_from_dat_wrap as *const ())?;
	}

	Ok(())
}

unsafe fn tour(a: *const (), b: *const ()) -> anyhow::Result<()> {
	let r = retour::RawDetour::new(a, b)?;
	r.enable()?;
	std::mem::forget(r);
	Ok(())
}

// read_from_dat has an unusual callconv that is like fastcall, but caller-cleanup.
#[naked]
unsafe extern "fastcall" fn read_from_dat_wrap(buf: *mut u8, archive_no: u32, offset: usize, length: usize) -> bool {
	std::arch::asm!{
		"push [esp+8]",
		"push [esp+8]",
		"call {read_from_dat}",
		"ret",
		read_from_dat = sym read_from_dat,
		options(noreturn)
	}
}

extern "fastcall" fn read_from_dat(buf: *mut u8, archive_no: u32, offset: usize, length: usize) -> bool {
	match do_read(buf, archive_no, offset, length) {
		Ok(()) => true,
		Err(e) => {
			println!("SoraData: read failed: {e:?}");
			false
		}
	}
}

fn do_read(buf: *mut u8, archive_no: u32, offset: usize, length: usize) -> anyhow::Result<()> {
	let index = ADDRS.dir_entries()[archive_no as usize].iter()
		.position(|e| e.offset == offset && e.asize != 0);
	let Some(index) = index else {
		anyhow::bail!("no match");
	};

	let name = normalize_name(&ADDRS.dir_entries()[archive_no as usize][index].name());
	let gamedir = PATH.parent().unwrap();
	let file = gamedir.join(format!("data/ED6_DT{archive_no:02x}/{name}"));
	println!("SoraData: {} ({length}B)", file.display());
	if file.exists() {
		println!("   exists");
		if is_raw(&name) {
			let data = std::fs::read(file)?;
			let buf = unsafe { std::slice::from_raw_parts_mut(buf, 0x600000) };
			fake_compress(&mut std::io::Cursor::new(buf), &data)?;
		} else {
			let mut f = std::fs::File::open(file)?;
			let buf = unsafe { std::slice::from_raw_parts_mut(buf, length) };
			f.read_exact(buf)?;
		}
		Ok(())
	} else {
		let dirfile = gamedir.join(format!("ED6_DT{archive_no:02x}.DAT"));
		let mut f = std::fs::File::open(dirfile)?;
		f.seek(SeekFrom::Start(offset as u64))?;
		let buf = unsafe { std::slice::from_raw_parts_mut(buf, length) };
		f.read_exact(buf)?;
		Ok(())
	}
}

fn fake_compress(buf: &mut impl Write, data: &[u8]) -> anyhow::Result<()> {
	let mut chunks = data.chunks((1<<16)-1).peekable();
	while let Some(chunk) = chunks.next() {
		buf.write_all(&u16::to_le_bytes((chunk.len() as u16).wrapping_add(2)))?;
		for sub in chunk.chunks((1<<13)-1) {
			if sub.len() >= 1<<5 {
				buf.write_all(&[1<<5 | (sub.len() >> 8) as u8])?;
			}
			buf.write_all(&[sub.len() as u8])?;
			buf.write_all(sub)?;
		}
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
	name.ends_with("._ds") || name.ends_with(".wav")
}
