// Copyright 2023 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

// use alloc::{string::String, vec};

use bytemuck::Pod;
use serde::de::{DeserializeOwned, DeserializeSeed, IntoDeserializer, Visitor};
use core::fmt::{Display, Formatter};

// use super::err::{Error, Result};
// use crate::align_up;

pub const WORD_SIZE: usize = 4;

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// A custom error
    Custom(String),
    /// Found a bool that wasn't 0 or 1
    DeserializeBadBool,
    /// Found an invalid unicode char
    DeserializeBadChar,
    /// Found an Option discriminant that wasn't 0 or 1
    DeserializeBadOption,
    /// Tried to parse invalid utf-8
    DeserializeBadUtf8,
    /// Unexpected end during deserialization
    DeserializeUnexpectedEnd,
    /// Not supported
    NotSupported,
    /// The serialize buffer is full
    SerializeBufferFull,
}

impl Display for Error {
    fn fmt(&self, formatter: &mut Formatter) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Custom(msg) => msg,
            Self::DeserializeBadBool => "Found a bool that wasn't 0 or 1",
            Self::DeserializeBadChar => "Found an invalid unicode char",
            Self::DeserializeBadOption => "Found an Option discriminant that wasn't 0 or 1",
            Self::DeserializeBadUtf8 => "Tried to parse invalid utf-8",
            Self::DeserializeUnexpectedEnd => "Unexpected end during deserialization",
            Self::NotSupported => "Not supported",
            Self::SerializeBufferFull => "The serialize buffer is full",
        })
    }
}

impl serde::ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

impl serde::de::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Custom(msg.to_string())
    }
}

// This is an alias for either std::Error, or serde's no_std error replacement.
impl serde::ser::StdError for Error {}

/// A Result type for `risc0_zkvm::serde` operations that can fail
pub type Result<T> = core::result::Result<T, Error>;

/// A reader for reading streams with serialized word-based data
pub trait WordRead {
    /// Fill the given buffer with words from input.  Returns an error if EOF
    /// was encountered.
    fn read_words(&mut self, words: &mut [u32]) -> Result<()>;

    /// Fill the given buffer with bytes from input, and discard the
    /// padding up to the next word boundary.  Returns an error if EOF was
    /// encountered.
    fn read_padded_bytes(&mut self, bytes: &mut [u8]) -> Result<()>;
}

// Allow borrowed WordReads to work transparently
impl<R: WordRead + ?Sized> WordRead for &mut R {
    fn read_words(&mut self, words: &mut [u32]) -> Result<()> {
        (**self).read_words(words)
    }

    fn read_padded_bytes(&mut self, bytes: &mut [u8]) -> Result<()> {
        (**self).read_padded_bytes(bytes)
    }
}

impl WordRead for &[u32] {
    fn read_words(&mut self, out: &mut [u32]) -> Result<()> {
        if out.len() > self.len() {
            Err(Error::DeserializeUnexpectedEnd)
        } else {
            out.clone_from_slice(&self[..out.len()]);
            (_, *self) = self.split_at(out.len());
            Ok(())
        }
    }

    fn read_padded_bytes(&mut self, out: &mut [u8]) -> Result<()> {
        let bytes: &[u8] = bytemuck::cast_slice(self);
        if out.len() > bytes.len() {
            Err(Error::DeserializeUnexpectedEnd)
        } else {
            out.clone_from_slice(&bytes[..out.len()]);
            (_, *self) = self.split_at(align_up(out.len(), WORD_SIZE) / WORD_SIZE);
            Ok(())
        }
    }
}

/// Deserialize a slice into the specified type.
///
/// Deserialize `slice` into type `T`. Returns an `Err` if deserialization isn't
/// possible, such as if `slice` is not the serialized form of an object of type
/// `T`.
pub fn from_slice<T: DeserializeOwned, P: Pod>(slice: &[P]) -> Result<T> {
    match bytemuck::try_cast_slice(slice) {
        Ok(slice) => {
            let mut deserializer = Deserializer::new(slice);
            T::deserialize(&mut deserializer)
        }
        Err(ref e) => panic!("failed to cast or read slice as [u32]: {}", e),
    }
}

/// Enables deserializing from a WordRead
pub struct Deserializer<'de, R: WordRead + 'de> {
    reader: R,
    phantom: core::marker::PhantomData<&'de ()>,
}

struct SeqAccess<'a, 'de, R: WordRead + 'de> {
    deserializer: &'a mut Deserializer<'de, R>,
    len: usize,
}


impl<'de, 'a, R: WordRead + 'de> serde::de::SeqAccess<'de> for SeqAccess<'a, 'de, R> {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
    where
        T: DeserializeSeed<'de>,
    {
        if self.len > 0 {
            self.len -= 1;
            Ok(Some(DeserializeSeed::deserialize(
                seed,
                &mut *self.deserializer,
            )?))
        } else {
            Ok(None)
        }
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len)
    }
}

impl<'de, 'a, R: WordRead + 'de> serde::de::VariantAccess<'de> for &'a mut Deserializer<'de, R> {
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<V::Value> {
        DeserializeSeed::deserialize(seed, self)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        serde::de::Deserializer::deserialize_tuple(self, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        serde::de::Deserializer::deserialize_tuple(self, fields.len(), visitor)
    }
}

impl<'de, 'a, R: WordRead + 'de> serde::de::EnumAccess<'de> for &'a mut Deserializer<'de, R> {
    type Error = Error;
    type Variant = Self;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self)> {
        let tag = self.try_take_word()?;
        let val = DeserializeSeed::deserialize(seed, tag.into_deserializer())?;
        Ok((val, self))
    }
}


struct MapAccess<'a, 'de, R: WordRead + 'de> {
    deserializer: &'a mut Deserializer<'de, R>,
    len: usize,
}

impl<'a, 'de: 'a, R: WordRead + 'de> serde::de::MapAccess<'de> for MapAccess<'a, 'de, R> {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.len > 0 {
            self.len -= 1;
            Ok(Some(DeserializeSeed::deserialize(
                seed,
                &mut *self.deserializer,
            )?))
        } else {
            Ok(None)
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        DeserializeSeed::deserialize(seed, &mut *self.deserializer)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(self.len)
    }
}

impl<'de, R: WordRead + 'de> Deserializer<'de, R> {
    /// Construct a Deserializer
    ///
    /// Creates a deserializer for deserializing from the given WordWred
    pub fn new(reader: R) -> Self {
        Deserializer {
            reader,
            phantom: core::marker::PhantomData,
        }
    }

    fn try_take_word(&mut self) -> Result<u32> {
        let mut val = 0u32;
        self.reader.read_words(core::slice::from_mut(&mut val))?;
        Ok(val)
    }

    fn try_take_dword(&mut self) -> Result<u64> {
        let low = self.try_take_word()? as u64;
        let high = self.try_take_word()? as u64;
        Ok(low | high << 32)
    }
}

impl<'de, 'a, R: WordRead + 'de> serde::Deserializer<'de> for &'a mut Deserializer<'de, R> {
    type Error = Error;

    fn is_human_readable(&self) -> bool {
        false
    }

    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::NotSupported)
    }

    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let val = match self.try_take_word()? {
            0 => false,
            1 => true,
            _ => return Err(Error::DeserializeBadBool),
        };
        visitor.visit_bool(val)
    }

    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.try_take_word()? as i32)
    }

    fn deserialize_i16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.try_take_word()? as i32)
    }

    fn deserialize_i32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i32(self.try_take_word()? as i32)
    }

    fn deserialize_i64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_i64(self.try_take_dword()? as i64)
    }

    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.try_take_word()?)
    }

    fn deserialize_u16<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.try_take_word()?)
    }

    fn deserialize_u32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u32(self.try_take_word()?)
    }

    fn deserialize_u64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_u64(self.try_take_dword()?)
    }

    fn deserialize_f32<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f32(f32::from_bits(self.try_take_word()?))
    }

    fn deserialize_f64<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_f64(f64::from_bits(self.try_take_dword()?))
    }

    fn deserialize_char<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let c = char::from_u32(self.try_take_word()?).ok_or(Error::DeserializeBadChar)?;
        visitor.visit_char(c)
    }

    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let len_bytes = self.try_take_word()? as usize;
        // TODO: Can we use MaybeUninit here instead of zeroing out?
        // The documentation for sys::io::Read implies that it's not
        // safe; is there another way to not do double writes here?
        let mut bytes = vec![0u8; len_bytes];
        self.reader.read_padded_bytes(&mut bytes)?;
        visitor.visit_string(String::from_utf8(bytes).map_err(|_| Error::DeserializeBadChar)?)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let len_bytes = self.try_take_word()? as usize;
        // TODO: Can we use MaybeUninit here instead of zeroing out?
        // The documentation for sys::io::Read implies that it's not
        // safe; is there another way to not do double writes here?
        let mut bytes = vec![0u8; len_bytes];
        self.reader.read_padded_bytes(&mut bytes)?;
        visitor.visit_byte_buf(bytes)
    }

    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_bytes(visitor)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        match self.try_take_word()? {
            0 => visitor.visit_none(),
            1 => visitor.visit_some(self),
            _ => Err(Error::DeserializeBadOption),
        }
    }

    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let len = self.try_take_word()? as usize;
        visitor.visit_seq(SeqAccess {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_seq(SeqAccess {
            deserializer: self,
            len,
        })
    }

    fn deserialize_tuple_struct<V>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        let len = self.try_take_word()? as usize;
        visitor.visit_map(MapAccess {
            deserializer: self,
            len,
        })
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        visitor.visit_enum(self)
    }

    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::NotSupported)
    }

    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
    where
        V: Visitor<'de>,
    {
        Err(Error::NotSupported)
    }
}
