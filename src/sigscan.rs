pub macro sig($($a:tt)*) {
	&[$(unit!($a)),*]
}

macro unit {
	(?) => { None },
	($a:literal) => { Some($a) },
	($($t:tt)*) => { compile_error!(stringify!($($t)*)) },
}

#[track_caller]
pub fn scan(sig: &[Option<u8>]) -> *const u8 {
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

pub macro sigscan($($a:tt)*) {
	scan(sig!($($a)*))
}
