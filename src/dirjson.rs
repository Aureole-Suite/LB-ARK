use std::{marker::PhantomData, str::FromStr};

use serde::{
	de::{self, Error, Unexpected},
	Deserialize, Deserializer,
};
use serde_with::DeserializeAs;

#[serde_with::serde_as]
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DirJson {
	#[serde(flatten)]
	#[serde_as(as = "serde_with::Map<_, StringOrStruct<_>>")]
	pub entries: Vec<(Key, Entry)>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum Key {
	Id(u32),
	Name(String),
}

impl std::fmt::Debug for Key {
	fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		match self {
			Self::Id(arg) => write!(f, "0x{arg:08X}"),
			Self::Name(arg) => write!(f, "{arg:?}"),
		}
	}
}

impl<'de> Deserialize<'de> for Key {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let s = String::deserialize(deserializer)?;
		if let Some(s) = s.strip_prefix("0x") {
			let id = u32::from_str_radix(s, 16)
				.map_err(|_| D::Error::invalid_value(Unexpected::Str(s), &"hex integer"))?;
			Ok(Key::Id(id))
		} else {
			Ok(Key::Name(s.to_owned()))
		}
	}
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Entry {
	pub name: Option<String>,
	pub path: String,
}

impl FromStr for Entry {
	type Err = std::convert::Infallible;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Entry {
			name: None,
			path: s.to_owned(),
		})
	}
}

pub struct StringOrStruct<T>(PhantomData<T>);

impl<'de, T, U> DeserializeAs<'de, T> for StringOrStruct<U>
where
	StringOrStructV<T, U>: de::Visitor<'de, Value = T>,
{
	fn deserialize_as<D: Deserializer<'de>>(de: D) -> Result<T, D::Error> {
		de.deserialize_any(StringOrStructV::<T, U>(PhantomData))
	}
}

struct StringOrStructV<T, U>(PhantomData<(T, U)>);

impl<'de, T, U> de::Visitor<'de> for StringOrStructV<T, U>
where
	U: DeserializeAs<'de, T>,
	T: FromStr,
	T::Err: std::fmt::Display,
{
	type Value = T;

	fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
		formatter.write_str("string or map")
	}

	fn visit_str<E: de::Error>(self, value: &str) -> Result<T, E> {
		T::from_str(value).map_err(de::Error::custom)
	}

	fn visit_map<M: de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
		U::deserialize_as(de::value::MapAccessDeserializer::new(map))
	}
}
