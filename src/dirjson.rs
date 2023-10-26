use serde::{de::Error, de::Unexpected, Deserialize, Deserializer};

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct DirJson(#[serde(deserialize_with = "pairs::deserialize")] pub Vec<(Key, Entry)>);

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
	pub name: Option<String>,
	pub path: String,
}

#[derive(serde::Deserialize)]
#[serde(remote = "Entry")]
struct EntryDef {
	#[serde(default)]
	pub name: Option<String>,
	pub path: String,
}

impl<'de> Deserialize<'de> for Entry {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		struct Vis;
		impl<'de> serde::de::Visitor<'de> for Vis {
			type Value = Entry;

			fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
				formatter.write_str("string or map")
			}

			fn visit_str<E>(self, value: &str) -> Result<Entry, E>
			where
				E: serde::de::Error,
			{
				Ok(Entry {
					name: None,
					path: value.to_owned(),
				})
			}

			fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<Entry, M::Error> {
				EntryDef::deserialize(serde::de::value::MapAccessDeserializer::new(map))
			}
		}

		deserializer.deserialize_any(Vis)
	}
}

mod pairs {
	use serde::de::{Deserialize, MapAccess, Visitor};
	use std::marker::PhantomData;

	pub fn deserialize<'de, K, V, D: serde::de::Deserializer<'de>>(
		deserializer: D,
	) -> Result<Vec<(K, V)>, D::Error>
	where
		K: Deserialize<'de>,
		V: Deserialize<'de>,
	{
		struct MyVisitor<K, V>(PhantomData<(K, V)>);

		impl<'d, K, V> Visitor<'d> for MyVisitor<K, V>
		where
			K: serde::Deserialize<'d>,
			V: serde::Deserialize<'d>,
		{
			type Value = Vec<(K, V)>;

			fn expecting(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
				f.write_str("map")
			}

			fn visit_map<M: MapAccess<'d>>(self, mut access: M) -> Result<Self::Value, M::Error> {
				let mut i = Vec::new();
				while let Some((key, value)) = access.next_entry()? {
					i.push((key, value))
				}
				Ok(i)
			}
		}
		Ok(deserializer.deserialize_map(MyVisitor(PhantomData))?)
	}
}
