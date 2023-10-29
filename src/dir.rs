use std::cell::Cell;
use std::sync::{LazyLock, Mutex};

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Entry {
	pub stored_name: [u8; 12],
	pub unk1: u32,
	pub csize: usize,
	pub unk2: u32,
	pub asize: usize,
	pub ts: u32,
	pub offset: usize,
}

impl std::fmt::Debug for Entry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.name().fmt(f)
	}
}

impl Default for Entry {
	fn default() -> Self {
		Self {
			stored_name: *b"/_______.___",
			unk1: 0,
			csize: 0,
			unk2: 0,
			asize: 0,
			ts: 0,
			offset: 0,
		}
	}
}

impl Entry {
	pub fn name(&self) -> String {
		let name = String::from_utf8_lossy(&self.stored_name).to_lowercase();
		if let Some((name, ext)) = name.split_once('.') {
			format!("{}.{ext}", name.trim_end_matches(' '))
		} else {
			name
		}
	}

	pub fn to_stored_name(name: &str) -> Option<[u8; 12]> {
		let (_, name) = name.rsplit_once(['/', '\\']).unwrap_or(("", name));
		let name = name.to_uppercase();
		let (name, ext) = name.split_once('.').unwrap_or((&name, ""));
		if name.len() > 8 || ext.len() > 3 {
			return None;
		}
		let mut o = *b"        .   ";
		o[..name.len()].copy_from_slice(name.as_bytes());
		o[9..][..ext.len()].copy_from_slice(ext.as_bytes());
		Some(o)
	}
}

pub struct Dirs {
	ptrs: &'static [Cell<*const Entry>; 64],
	lens: &'static [Cell<usize>; 64],
	entries: [Vec<Entry>; 64],
}

unsafe impl Send for Dirs {}

impl Dirs {
	#[allow(clippy::missing_safety_doc)] // I don't care
	pub unsafe fn new() -> Dirs {
		let n = crate::sigscan::sigscan! {
			0x89 0x34 0xBD ? ? ? ?  // mov dword ptr [edi*4 + dir_n_entries], esi
			0x81 0xC3 ? ? ? ?       // add ebx, ? ; 36*number of entries: 2047 in FC, 4096 in SC/3rd
			0x89 0x04 0xBD ? ? ? ?  // mov dword ptr [edi*4 + dir_entries], eax
			0x47                    // inc edi
		};
		let lens = &*std::ptr::read_unaligned(n.add(3) as *const *const _);
		let ptrs = &*std::ptr::read_unaligned(n.add(16) as *const *const _);
		let entries = std::array::from_fn(|_| Vec::new());
		Dirs {
			lens,
			ptrs,
			entries,
		}
	}

	pub fn entries(&self) -> [&[Entry]; 64] {
		std::array::from_fn(|arc| unsafe {
			if self.ptrs[arc].get().is_null() {
				&[]
			} else {
				std::slice::from_raw_parts(self.ptrs[arc].get(), self.lens[arc].get())
			}
		})
	}

	pub fn entries_mut(&mut self) -> [&mut [Entry]; 64] {
		std::array::from_fn(|arc| unsafe {
			if self.ptrs[arc].get().is_null() {
				&mut []
			} else {
				std::slice::from_raw_parts_mut(self.ptrs[arc].get() as *mut _, self.lens[arc].get())
			}
		})
	}

	pub fn get(&mut self, arc: u8, idx: u16) -> &mut Entry {
		assert!(arc < 64);
		let arc = arc as usize;
		let idx = idx as usize;

		let ptr = self.ptrs[arc].get();
		let len = self.lens[arc].get();
		let entries = &mut self.entries[arc];
		if !ptr.is_null() {
			if ptr != entries.as_ptr() {
				entries.clear();
				entries.extend_from_slice(unsafe { std::slice::from_raw_parts(ptr, len) });
			}
			unsafe {
				entries.set_len(len);
			}
		}

		while entries.len() <= idx {
			entries.push(Entry::default());
		}
		self.ptrs[arc].set(entries.as_ptr());
		self.lens[arc].set(entries.len());

		&mut entries[idx]
	}
}

pub static DIRS: LazyLock<Mutex<Dirs>> = LazyLock::new(|| Mutex::new(unsafe { Dirs::new() }));
