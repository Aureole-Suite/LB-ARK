use std::ffi::OsString;
use std::path::{PathBuf, Path};

use eyre_span::ReportSpan;
use windows::core::HSTRING;
use windows::Win32::{
	Foundation::HMODULE,
	System::LibraryLoader::GetModuleFileNameW,
	UI::WindowsAndMessaging::{MessageBoxW, MESSAGEBOX_STYLE},
};

pub fn windows_path(f: impl FnOnce(&mut [u16]) -> u32) -> PathBuf {
	use std::os::windows::ffi::OsStringExt;
	let mut path = [0; 260];
	let n = f(&mut path);
	let start = if path.starts_with(&b"\\\\?\\".map(|a| a as u16)) { 4 } else { 0 };
	let path = OsString::from_wide(&path[start..n as usize]);
	PathBuf::from(path)
}

pub fn msgbox(title: &str, body: &str, style: u32) -> u32 {
	unsafe {
		MessageBoxW(
			None,
			&HSTRING::from(body),
			&HSTRING::from(title),
			MESSAGEBOX_STYLE(style)
		).0 as u32
	}
}

/// Shows an error in an appropriate way, returning the value as an `Option`.
pub fn catch<T>(a: eyre::Result<T>) -> Option<T> {
	match a {
		Ok(v) => Some(v),
		Err(e) => {
			e.span().in_scope(|| tracing::error!("{e}"));
			msgbox("LB-ARK error", &format!("{e:#}"), 0x10);
			None
		}
	}
}

/// Converts the path to be relative to the game directory, for nicer error messages.
pub fn rel(path: &Path) -> std::path::Display {
	path.strip_prefix(*GAME_DIR).unwrap_or(path).display()
}

lazy_static::lazy_static! {
	/// Path to the main executable, generally named `ed6_win_something.exe`.
	pub static ref EXE_PATH: PathBuf = windows_path(|p| unsafe { GetModuleFileNameW(HMODULE(0), p) });
	/// Path to the game directory, where all game files are located.
	pub static ref GAME_DIR: &'static Path = EXE_PATH.parent().unwrap();
	/// Path to LB-DIR data directory, where LB-DIR reads the files from.
	pub static ref DATA_DIR: PathBuf = GAME_DIR.join("data");
}

pub fn has_extension(path: &Path, ext: &str) -> bool {
	match try { path.extension()?.to_str()?.to_lowercase() } {
		Some(t) => t == ext,
		None => false,
	}
}
