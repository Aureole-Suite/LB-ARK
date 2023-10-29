use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::sync::LazyLock;

use camino::{Utf8Path, Utf8PathBuf};
use eyre_span::ReportSpan;

use windows::core::HSTRING;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
	GetFinalPathNameByHandleW, SetFilePointer, FILE_CURRENT, VOLUME_NAME_DOS,
};
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MESSAGEBOX_STYLE};

pub fn file_pos(handle: HANDLE) -> (Utf8PathBuf, usize) {
	let n = unsafe { GetFinalPathNameByHandleW(handle, &mut [], VOLUME_NAME_DOS) } as usize;
	let mut path = vec![0; n];
	let n = unsafe { GetFinalPathNameByHandleW(handle, &mut path, VOLUME_NAME_DOS) } as usize;
	let path = OsString::from_wide(&path[..n]);

	let path = std::path::PathBuf::from(path).try_into().unwrap();

	let pos = unsafe { SetFilePointer(handle, 0, None, FILE_CURRENT) } as usize;
	(path, pos)
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

/// Path to the main executable, generally named `ed6_win_something.exe`.
pub static EXE_PATH: LazyLock<Utf8PathBuf> =
	LazyLock::new(|| {
		let exe = std::env::current_exe().unwrap();
		let exe = std::fs::canonicalize(exe).unwrap();
		exe.try_into().unwrap()
	});
/// Path to the game directory, where all game files are located.
pub static GAME_DIR: LazyLock<&'static Utf8Path> = LazyLock::new(|| EXE_PATH.parent().unwrap());
/// Path to LB-DIR data directory, where LB-DIR reads the files from.
pub static DATA_DIR: LazyLock<Utf8PathBuf> = LazyLock::new(|| GAME_DIR.join("data"));

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
