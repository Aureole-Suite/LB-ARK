use std::ffi::OsString;

use camino::{Utf8Path, Utf8PathBuf};
use eyre_span::ReportSpan;
use windows::core::HSTRING;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MESSAGEBOX_STYLE};

pub fn windows_path(f: impl FnOnce(&mut [u16]) -> u32) -> Utf8PathBuf {
	use std::os::windows::ffi::OsStringExt;
	let mut path = [0; 260];
	let n = f(&mut path);
	let start = if path.starts_with(&b"\\\\?\\".map(|a| a as u16)) {
		4
	} else {
		0
	};
	let path = OsString::from_wide(&path[start..n as usize]);
	std::path::PathBuf::from(path).try_into().unwrap()
}

pub fn msgbox(title: &str, body: &str, style: u32) -> u32 {
	unsafe {
		MessageBoxW(
			None,
			&HSTRING::from(body),
			&HSTRING::from(title),
			MESSAGEBOX_STYLE(style),
		)
		.0 as u32
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
pub fn rel(path: &Utf8Path) -> &Utf8Path {
	path.strip_prefix(*GAME_DIR).unwrap_or(path)
}

lazy_static::lazy_static! {
	/// Path to the main executable, generally named `ed6_win_something.exe`.
	pub static ref EXE_PATH: Utf8PathBuf = std::env::current_exe().unwrap().try_into().unwrap();
	/// Path to the game directory, where all game files are located.
	pub static ref GAME_DIR: &'static Utf8Path = EXE_PATH.parent().unwrap();
	/// Path to LB-DIR data directory, where LB-DIR reads the files from.
	pub static ref DATA_DIR: Utf8PathBuf = GAME_DIR.join("data");
}

pub fn list_files(path: &Utf8Path, ext: &str) -> std::io::Result<Vec<Utf8PathBuf>> {
	let mut files = Vec::new();
	for file in path.read_dir_utf8()? {
		let path = file?.path().to_owned();
		if path
			.extension()
			.is_some_and(|e| e.eq_ignore_ascii_case(ext))
		{
			files.push(path)
		}
	}
	Ok(files)
}
