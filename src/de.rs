//! Deserialize ROSMSG binary data to a Rust data structure.
//!
//! Data types supported by ROSMSG are supported as well. This results in the
//! lack of support for:
//!
//! * Enums of any type, including `Option`
//! * `char`, so use one character `String`s instead
//! * Maps that can't be boiled down to `<String, String>`
//!
//! Any methods for blindly identifying structure are not supported, because
//! the data does not contain any type information.

use byteorder::{LittleEndian, ReadBytesExt};
use serde::de;
use super::error::{Error, ErrorKind, Result, ResultExt};
use std::io;

/// A structure for deserializing ROSMSG into Rust values.
///
/// The structure does not read the object size prefix.
/// It's the user's responsibility to pass the expected object size themselves.
///
/// Prefer using `from_reader`, `from_slice` and `from_str`.
pub struct Deserializer<R> {
    reader: R,
    length: u32,
}

impl<R> Deserializer<R>
    where R: io::Read
{
    /// Create a new ROSMSG deserializer.
    ///
    /// The value of `expected_length` tells the deserializer how long the data
    /// that we want to read is.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate serde_rosmsg;
    /// # use serde_rosmsg::de::Deserializer;
    /// # extern crate serde;
    /// # fn main() {
    /// use serde::de::Deserialize;
    ///
    /// let data = b"\x0d\0\0\0Hello, World!\xAE";
    /// let length = data.len();
    /// let cursor = std::io::Cursor::new(&data);
    /// let mut de = Deserializer::new(cursor, length as u32);
    /// assert_eq!(String::deserialize(&mut de).unwrap(), "Hello, World!");
    /// assert_eq!(u8::deserialize(&mut de).unwrap(), 0xAE);
    /// # }
    /// ```
    pub fn new(reader: R, expected_length: u32) -> Self {
        Deserializer {
            reader: reader,
            length: expected_length,
        }
    }

    /// Unwrap the `Reader` from the `Deserializer`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate serde_rosmsg;
    /// # use serde_rosmsg::de::Deserializer;
    /// # extern crate serde;
    /// # fn main() {
    /// use serde::de::Deserialize;
    ///
    /// let data = [2, 4, 8, 16];
    /// let cursor = std::io::Cursor::new(&data);
    /// let mut de = Deserializer::new(cursor, 2);
    /// assert_eq!(u16::deserialize(&mut de).unwrap(), 1026);
    /// let cursor_new = de.into_inner();
    /// let mut de_new = Deserializer::new(cursor_new, 2);
    /// assert_eq!(u16::deserialize(&mut de_new).unwrap(), 4104);
    /// # }
    /// ```
    pub fn into_inner(self) -> R {
        self.reader
    }

    /// Check if the deserializer is fully read.
    ///
    /// If this is true, one cannot read from the deserializer anymore, and
    /// should use a different deserializer.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate serde_rosmsg;
    /// # use serde_rosmsg::de::Deserializer;
    /// # extern crate serde;
    /// # fn main() {
    /// use serde::de::Deserialize;
    ///
    /// let data = [2, 4, 8, 16];
    /// let mut de = Deserializer::new(std::io::Cursor::new(&data), 4);
    /// assert_eq!(de.is_fully_read(), false);  // Still 4 bytes left to read
    /// u16::deserialize(&mut de).unwrap();     // Read 2 bytes
    /// assert_eq!(de.is_fully_read(), false);  // Still 2 bytes left to read
    /// u16::deserialize(&mut de).unwrap();     // Read 2 bytes
    /// assert_eq!(de.is_fully_read(), true);   // No more bytes left to read
    /// u16::deserialize(&mut de).unwrap_err(); // Failure to read more
    /// # }
    /// ```
    pub fn is_fully_read(&self) -> bool {
        self.length == 0
    }

    #[inline]
    fn reserve_bytes(&mut self, size: u32) -> Result<()> {
        if size > self.length {
            bail!(ErrorKind::Overflow);
        }
        self.length -= size;
        Ok(())
    }

    #[inline]
    fn pop_length(&mut self) -> Result<u32> {
        self.reserve_bytes(4)?;
        self.reader
            .read_u32::<LittleEndian>()
            .chain_err(|| ErrorKind::EndOfBuffer)
    }

    #[inline]
    fn get_string(&mut self) -> Result<String> {
        let length = self.pop_length()?;
        self.reserve_bytes(length)?;
        let mut buffer = vec![0; length as usize];
        self.reader
            .read_exact(&mut buffer)
            .chain_err(|| ErrorKind::EndOfBuffer)?;
        String::from_utf8(buffer).chain_err(|| ErrorKind::BadStringData)
    }

    fn get_bytes(&mut self) -> Result<Vec<u8>> {
        let length = self.pop_length()?;
        self.reserve_bytes(length)?;
        let mut buffer = vec![0; length as usize];
        self.reader
            .read_exact(&mut buffer)
            .chain_err(|| ErrorKind::EndOfBuffer)?;
        Ok(buffer)
    }
}

macro_rules! impl_nums {
    ($ty:ty, $dser_method:ident, $visitor_method:ident, $reader_method:ident, $bytes:expr) => {
        #[inline]
        fn $dser_method<V>(self, visitor: V) -> Result<V::Value>
            where V: de::Visitor<'de>,
        {
            self.reserve_bytes($bytes)?;
            let value = self.reader.$reader_method::<LittleEndian>()
                .chain_err(|| ErrorKind::EndOfBuffer)?;
            visitor.$visitor_method(value)
        }
    }
}

impl<'de, 'a, R: io::Read> de::Deserializer<'de> for &'a mut Deserializer<R> {
    type Error = Error;

    #[inline]
    fn deserialize_any<V>(self, _visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedDeserializerMethod("deserialize_any".into()))
    }

    #[inline]
    fn deserialize_identifier<V>(self, _visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedDeserializerMethod("deserialize_identifier".into()))
    }

    #[inline]
    fn deserialize_bool<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        self.reserve_bytes(1)?;
        let value = self.reader
            .read_u8()
            .chain_err(|| ErrorKind::EndOfBuffer)
            .map(|v| v != 0)?;
        visitor.visit_bool(value)
    }

    #[inline]
    fn deserialize_u8<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        self.reserve_bytes(1)?;
        let value = self.reader
            .read_u8()
            .chain_err(|| ErrorKind::EndOfBuffer)?;
        visitor.visit_u8(value)
    }

    #[inline]
    fn deserialize_i8<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        self.reserve_bytes(1)?;
        let value = self.reader
            .read_i8()
            .chain_err(|| ErrorKind::EndOfBuffer)?;
        visitor.visit_i8(value)
    }

    impl_nums!(u16, deserialize_u16, visit_u16, read_u16, 2);
    impl_nums!(u32, deserialize_u32, visit_u32, read_u32, 4);
    impl_nums!(u64, deserialize_u64, visit_u64, read_u64, 8);
    impl_nums!(i16, deserialize_i16, visit_i16, read_i16, 2);
    impl_nums!(i32, deserialize_i32, visit_i32, read_i32, 4);
    impl_nums!(i64, deserialize_i64, visit_i64, read_i64, 8);
    impl_nums!(f32, deserialize_f32, visit_f32, read_f32, 4);
    impl_nums!(f64, deserialize_f64, visit_f64, read_f64, 8);

    #[inline]
    fn deserialize_char<V>(self, _visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedCharType)
    }

    #[inline]
    fn deserialize_str<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_str(&self.get_string()?)
    }

    #[inline]
    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_string(self.get_string()?)
    }

    #[inline]
    fn deserialize_bytes<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_byte_buf(self.get_bytes()?)
    }

    #[inline]
    fn deserialize_byte_buf<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_byte_buf(self.get_bytes()?)
    }

    #[inline]
    fn deserialize_option<V>(self, _visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedEnumType)
    }

    #[inline]
    fn deserialize_unit<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_unit()
    }

    #[inline]
    fn deserialize_unit_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_unit()
    }

    #[inline]
    fn deserialize_newtype_struct<V>(self, _name: &'static str, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        visitor.visit_newtype_struct(self)
    }

    #[inline]
    fn deserialize_seq<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        let len = self.pop_length()? as usize;

        struct Access<'a, R: io::Read + 'a> {
            deserializer: &'a mut Deserializer<R>,
            len: usize,
        }

        impl<'de, 'a, 'b: 'a, R: io::Read + 'b> de::SeqAccess<'de> for Access<'a, R> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
                where T: de::DeserializeSeed<'de>
            {
                if self.len > 0 {
                    self.len -= 1;
                    Ok(Some(seed.deserialize(&mut *self.deserializer)?))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        visitor.visit_seq(Access {
                              deserializer: self,
                              len: len,
                          })
    }

    #[inline]
    fn deserialize_tuple<V>(self, len: usize, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        struct Access<'a, R: io::Read + 'a> {
            deserializer: &'a mut Deserializer<R>,
            len: usize,
        }

        impl<'de, 'a, 'b: 'a, R: io::Read + 'b> de::SeqAccess<'de> for Access<'a, R> {
            type Error = Error;

            fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>>
                where T: de::DeserializeSeed<'de>
            {
                if self.len > 0 {
                    self.len -= 1;
                    Ok(Some(seed.deserialize(&mut *self.deserializer)?))
                } else {
                    Ok(None)
                }
            }

            fn size_hint(&self) -> Option<usize> {
                Some(self.len)
            }
        }

        visitor.visit_seq(Access {
                              deserializer: self,
                              len: len,
                          })
    }

    #[inline]
    fn deserialize_tuple_struct<V>(self,
                                   _name: &'static str,
                                   len: usize,
                                   visitor: V)
                                   -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        self.deserialize_tuple(len, visitor)
    }

    #[inline]
    fn deserialize_map<V>(self, visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        struct Access<'a, R: io::Read + 'a> {
            deserializer: &'a mut Deserializer<R>,
            key: Vec<u8>,
            value: Vec<u8>,
        }

        impl<'a, R: io::Read + 'a> Access<'a, R> {
            #[inline]
            fn pop_item(&mut self) -> Result<()> {
                let data = self.deserializer.get_string()?;
                let mut data = data.splitn(2, '=');
                self.key = match data.next() {
                    Some(v) => Self::value_into_bytes(v)?,
                    None => bail!(ErrorKind::BadMapEntry),
                };
                self.value = match data.next() {
                    Some(v) => Self::value_into_bytes(v)?,
                    None => bail!(ErrorKind::BadMapEntry),
                };
                Ok(())
            }

            #[inline]
            fn value_into_bytes(val: &str) -> Result<Vec<u8>> {
                use super::Serializer;
                use serde::Serialize;
                let mut answer = Vec::<u8>::new();
                val.serialize(&mut Serializer::new(&mut answer))?;
                Ok(answer)
            }
        }

        impl<'de, 'a, 'b: 'a, R: io::Read + 'b> de::MapAccess<'de> for Access<'a, R> {
            type Error = Error;

            #[inline]
            fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>>
                where K: de::DeserializeSeed<'de>
            {
                if self.deserializer.is_fully_read() {
                    Ok(None)
                } else {
                    self.pop_item()?;
                    let mut deserializer = Deserializer::new(io::Cursor::new(&self.key),
                                                             self.key.len() as u32);
                    Ok(Some(seed.deserialize(&mut deserializer)?))
                }
            }

            #[inline]
            fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value>
                where V: de::DeserializeSeed<'de>
            {
                let mut deserializer = Deserializer::new(io::Cursor::new(&self.value),
                                                         self.value.len() as u32);
                Ok(seed.deserialize(&mut deserializer)?)
            }
        }

        visitor.visit_map(Access {
                              deserializer: self,
                              key: Vec::new(),
                              value: Vec::new(),
                          })
    }

    #[inline]
    fn deserialize_struct<V>(self,
                             _name: &'static str,
                             fields: &'static [&'static str],
                             visitor: V)
                             -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        self.deserialize_tuple(fields.len(), visitor)
    }

    #[inline]
    fn deserialize_enum<V>(self,
                           _name: &'static str,
                           _variants: &'static [&'static str],
                           _visitor: V)
                           -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedEnumType)
    }

    #[inline]
    fn deserialize_ignored_any<V>(self, _visitor: V) -> Result<V::Value>
        where V: de::Visitor<'de>
    {
        bail!(ErrorKind::UnsupportedDeserializerMethod("deserialize_ignored_any".into()))
    }
}

impl de::Error for Error {
    #[inline]
    fn custom<T: ::std::fmt::Display>(msg: T) -> Self {
        format!("{}", msg).into()
    }
}

/// Deserialize an instance of type `T` from an IO stream of ROSMSG data.
///
/// This conversion can fail if the passed stream of bytes does not match the
/// structure expected by `T`. It can also fail if the structure contains
/// unsupported elements.
///
/// # Examples
///
/// ```rust
/// # use serde_rosmsg::de::from_reader;
/// # use std;
/// let data = [
///     17, 0, 0, 0,
///     13, 0, 0, 0,
///     72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33];
/// let mut cursor = std::io::Cursor::new(&data);
/// let value: String = from_reader(&mut cursor).unwrap();
/// assert_eq!(value, "Hello, World!");
///
/// let data = [4, 0, 0, 0, 2, 4, 8, 16];
/// let mut cursor = std::io::Cursor::new(&data);
/// let value: (u16, u16) = from_reader(&mut cursor).unwrap();
/// assert_eq!(value, (1026, 4104));
/// ```
pub fn from_reader<'de, R, T>(mut reader: R) -> Result<T>
    where R: io::Read,
          T: de::Deserialize<'de>
{
    let length = reader.read_u32::<LittleEndian>()?;
    let mut deserializer = Deserializer::new(reader, length);
    let value = T::deserialize(&mut deserializer)?;
    if !deserializer.is_fully_read() {
        bail!(ErrorKind::Underflow);
    }
    Ok(value)
}

/// Deserialize an instance of type `T` from bytes of ROSMSG data.
///
/// This conversion can fail if the passed stream of bytes does not match the
/// structure expected by `T`. It can also fail if the structure contains
/// unsupported elements.
///
/// # Examples
///
/// ```rust
/// # use serde_rosmsg::de::from_slice;
/// let value: String = from_slice(&[
///     17, 0, 0, 0,
///     13, 0, 0, 0,
///     72, 101, 108, 108, 111, 44, 32, 87, 111, 114, 108, 100, 33]).unwrap();
/// assert_eq!(value, "Hello, World!");
///
/// let value: (u16, u16) = from_slice(&[4, 0, 0, 0, 2, 4, 8, 16]).unwrap();
/// assert_eq!(value, (1026, 4104));
/// ```
pub fn from_slice<'de, T>(bytes: &[u8]) -> Result<T>
    where T: de::Deserialize<'de>
{
    from_reader(io::Cursor::new(bytes))
}

/// Deserialize an instance of type `T` from a string of ROSMSG data.
///
/// This conversion can fail if the passed stream of bytes does not match the
/// structure expected by `T`. It can also fail if the structure contains
/// unsupported elements.
///
/// # Examples
///
/// ```rust
/// # use serde_rosmsg::de::from_str;
/// let value: String = from_str("\x11\0\0\0\x0d\0\0\0Hello, World!").unwrap();
/// assert_eq!(value, "Hello, World!");
///
/// let value: (u16, u16) = from_str("\x04\0\0\0\x02\x04\x08\x10").unwrap();
/// assert_eq!(value, (1026, 4104));
/// ```
pub fn from_str<'de, T>(value: &str) -> Result<T>
    where T: de::Deserialize<'de>
{
    from_slice(value.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std;

    #[test]
    fn reads_u8() {
        let data = vec![1, 0, 0, 0, 150];
        assert_eq!(150u8, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_u16() {
        let data = vec![2, 0, 0, 0, 0x34, 0xA2];
        assert_eq!(0xA234u16, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_u32() {
        let data = vec![4, 0, 0, 0, 0x45, 0x23, 1, 0xCD];
        assert_eq!(0xCD012345u32, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_u64() {
        let data = vec![8, 0, 0, 0, 0xBB, 0xAA, 0x10, 0x32, 0x54, 0x76, 0x98, 0xAB];
        assert_eq!(0xAB9876543210AABBu64, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_i8() {
        let data = vec![1, 0, 0, 0, 156];
        assert_eq!(-100i8, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_i16() {
        let data = vec![2, 0, 0, 0, 0xD0, 0x8A];
        assert_eq!(-30000i16, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_i32() {
        let data = vec![4, 0, 0, 0, 0x00, 0x6C, 0xCA, 0x88];
        assert_eq!(-2000000000i32, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_i64() {
        let data = vec![8, 0, 0, 0, 0x00, 0x00, 0x7c, 0x1d, 0xaf, 0x93, 0x19, 0x83];
        assert_eq!(-9000000000000000000i64, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_f32() {
        let data = vec![4, 0, 0, 0, 0x00, 0x70, 0x7b, 0x44];
        assert_eq!(1005.75f32, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_f64() {
        let data = vec![8, 0, 0, 0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x6e, 0x8f, 0x40];
        assert_eq!(1005.75f64, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_bool() {
        let data = vec![1, 0, 0, 0, 1];
        assert_eq!(true, from_slice(&data).unwrap());
        let data = vec![1, 0, 0, 0, 0];
        assert_eq!(false, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_bool_from_string() {
        assert_eq!(true, from_str("\x01\0\0\0\x01").unwrap());
        assert_eq!(false, from_str("\x01\0\0\0\x00").unwrap());
    }

    #[test]
    fn reads_string() {
        let data = vec![4, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!("", from_slice::<String>(&data).unwrap());
        let data = vec![17, 0, 0, 0, 13, 0, 0, 0, 72, 101, 108, 108, 111, 44, 32, 87, 111, 114,
                        108, 100, 33];
        assert_eq!("Hello, World!", from_slice::<String>(&data).unwrap());
    }

    #[test]
    fn reads_string_from_string() {
        assert_eq!("", from_str::<String>("\x04\0\0\0\0\0\0\0").unwrap());
        assert_eq!("Hello, World!",
                   from_str::<String>("\x11\0\0\0\x0d\0\0\0Hello, World!").unwrap());
    }

    #[test]
    fn reads_array() {
        let data = vec![8, 0, 0, 0, 7, 0, 1, 4, 33, 0, 57, 0];
        assert_eq!([7, 1025, 33, 57], from_slice::<[i16; 4]>(&data).unwrap());
    }

    #[test]
    fn reads_array_from_string() {
        assert_eq!([7, 1025, 32, 65],
                   from_str::<[i16; 4]>("\x08\0\0\0\x07\0\x01\x04 \0A\0").unwrap());
    }

    #[test]
    fn reads_array_struct() {
        #[derive(Debug,Deserialize,PartialEq)]
        struct TestArray([i16; 4]);
        let data = vec![8, 0, 0, 0, 7, 0, 1, 4, 33, 0, 57, 0];
        assert_eq!(TestArray([7, 1025, 33, 57]), from_slice(&data).unwrap());
    }

    #[test]
    fn reads_tuple_struct() {
        #[derive(Debug,Deserialize,PartialEq)]
        struct TestTuple(i16, bool, u8, String);
        let data = vec![14, 0, 0, 0, 2, 8, 1, 7, 6, 0, 0, 0, 65, 66, 67, 48, 49, 50];
        assert_eq!(TestTuple(2050, true, 7, String::from("ABC012")),
                   from_slice(&data).unwrap());
    }

    #[test]
    fn reads_vector() {
        let data = vec![12, 0, 0, 0, 4, 0, 0, 0, 7, 0, 1, 4, 33, 0, 57, 0];
        assert_eq!(vec![7, 1025, 33, 57],
                   from_slice::<Vec<i16>>(&data).unwrap());
    }

    #[derive(Debug,Deserialize,PartialEq)]
    struct TestStructOne {
        a: i16,
        b: bool,
        c: u8,
        d: String,
        e: Vec<bool>,
    }

    #[test]
    fn reads_simple_struct() {
        let v = TestStructOne {
            a: 2050i16,
            b: true,
            c: 7u8,
            d: String::from("ABC012"),
            e: vec![true, false, false, true],
        };
        let data = vec![22, 0, 0, 0, 2, 8, 1, 7, 6, 0, 0, 0, 65, 66, 67, 48, 49, 50, 4, 0, 0, 0,
                        1, 0, 0, 1];
        assert_eq!(v, from_slice(&data).unwrap());
    }

    #[derive(Debug,Deserialize,PartialEq)]
    struct TestStructPart {
        a: String,
        b: bool,
    }

    #[derive(Debug,Deserialize,PartialEq)]
    struct TestStructBig {
        a: Vec<TestStructPart>,
        b: String,
    }

    #[test]
    fn reads_complex_struct() {
        let mut parts = Vec::new();
        parts.push(TestStructPart {
                       a: String::from("ABC"),
                       b: true,
                   });
        parts.push(TestStructPart {
                       a: String::from("1!!!!"),
                       b: true,
                   });
        parts.push(TestStructPart {
                       a: String::from("234b"),
                       b: false,
                   });
        let v = TestStructBig {
            a: parts,
            b: String::from("EEe"),
        };
        let data = vec![38, 0, 0, 0, 3, 0, 0, 0, 3, 0, 0, 0, 65, 66, 67, 1, 5, 0, 0, 0, 49, 33,
                        33, 33, 33, 1, 4, 0, 0, 0, 50, 51, 52, 98, 0, 3, 0, 0, 0, 69, 69, 101];
        assert_eq!(v, from_slice(&data).unwrap());
    }

    #[test]
    fn reads_empty_string_string_map() {
        let input = vec![0, 0, 0, 0];
        let data = from_slice::<std::collections::HashMap<String, String>>(&input).unwrap();
        assert_eq!(0, data.len());
    }

    #[test]
    fn reads_single_element_string_string_map() {
        let input = vec![11, 0, 0, 0, 7, 0, 0, 0, 97, 98, 99, 61, 49, 50, 51];
        let data = from_slice::<std::collections::HashMap<String, String>>(&input).unwrap();
        assert_eq!(1, data.len());
        assert_eq!(Some(&String::from("123")), data.get("abc"));
    }

    #[test]
    fn reads_typical_header() {
        let input = vec![0xb0, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x6d, 0x65, 0x73, 0x73,
                         0x61, 0x67, 0x65, 0x5f, 0x64, 0x65, 0x66, 0x69, 0x6e, 0x69, 0x74, 0x69,
                         0x6f, 0x6e, 0x3d, 0x73, 0x74, 0x72, 0x69, 0x6e, 0x67, 0x20, 0x64, 0x61,
                         0x74, 0x61, 0x0a, 0x0a, 0x25, 0x00, 0x00, 0x00, 0x63, 0x61, 0x6c, 0x6c,
                         0x65, 0x72, 0x69, 0x64, 0x3d, 0x2f, 0x72, 0x6f, 0x73, 0x74, 0x6f, 0x70,
                         0x69, 0x63, 0x5f, 0x34, 0x37, 0x36, 0x37, 0x5f, 0x31, 0x33, 0x31, 0x36,
                         0x39, 0x31, 0x32, 0x37, 0x34, 0x31, 0x35, 0x35, 0x37, 0x0a, 0x00, 0x00,
                         0x00, 0x6c, 0x61, 0x74, 0x63, 0x68, 0x69, 0x6e, 0x67, 0x3d, 0x31, 0x27,
                         0x00, 0x00, 0x00, 0x6d, 0x64, 0x35, 0x73, 0x75, 0x6d, 0x3d, 0x39, 0x39,
                         0x32, 0x63, 0x65, 0x38, 0x61, 0x31, 0x36, 0x38, 0x37, 0x63, 0x65, 0x63,
                         0x38, 0x63, 0x38, 0x62, 0x64, 0x38, 0x38, 0x33, 0x65, 0x63, 0x37, 0x33,
                         0x63, 0x61, 0x34, 0x31, 0x64, 0x31, 0x0e, 0x00, 0x00, 0x00, 0x74, 0x6f,
                         0x70, 0x69, 0x63, 0x3d, 0x2f, 0x63, 0x68, 0x61, 0x74, 0x74, 0x65, 0x72,
                         0x14, 0x00, 0x00, 0x00, 0x74, 0x79, 0x70, 0x65, 0x3d, 0x73, 0x74, 0x64,
                         0x5f, 0x6d, 0x73, 0x67, 0x73, 0x2f, 0x53, 0x74, 0x72, 0x69, 0x6e, 0x67];
        let data = from_slice::<std::collections::HashMap<String, String>>(&input).unwrap();
        assert_eq!(6, data.len());
        assert_eq!(Some(&String::from("string data\n\n")),
                   data.get("message_definition"));
        assert_eq!(Some(&String::from("/rostopic_4767_1316912741557")),
                   data.get("callerid"));
        assert_eq!(Some(&String::from("1")), data.get("latching"));
        assert_eq!(Some(&String::from("992ce8a1687cec8c8bd883ec73ca41d1")),
                   data.get("md5sum"));
        assert_eq!(Some(&String::from("/chatter")), data.get("topic"));
        assert_eq!(Some(&String::from("std_msgs/String")), data.get("type"));
    }

    #[test]
    fn reports_end_of_buffer() {
        let data = vec![4, 0, 0, 0, 0x45, 0x23, 1];
        let error = from_slice::<u32>(&data).unwrap_err();
        match *error.kind() {
            ErrorKind::EndOfBuffer => {}
            _ => panic!("End of buffer error expected, got: {:?}", error),
        }
    }

    #[test]
    fn reports_attempt_to_read_beyond_prediction() {
        let data = vec![2, 0, 0, 0, 0x45, 0x23, 1, 0xCD];
        let error = from_slice::<u32>(&data).unwrap_err();
        match *error.kind() {
            ErrorKind::Overflow => {}
            _ => panic!("Overflow error expected, got: {:?}", error),
        }
    }

    #[test]
    fn reports_failure_to_read_predicted_length() {
        let data = vec![5, 0, 0, 0, 0x45, 0x23, 1, 0xCD];
        let error = from_slice::<u32>(&data).unwrap_err();
        match *error.kind() {
            ErrorKind::Underflow => {}
            _ => panic!("Underflow error expected, got: {:?}", error),
        }
    }

    #[test]
    fn requires_right_length_for_vector() {
        let data = vec![12, 0, 0, 0, 3, 0, 0, 0, 7, 0, 1, 4, 33, 0, 57, 0];
        from_slice::<Vec<i16>>(&data).unwrap_err();
        let data = vec![12, 0, 0, 0, 5, 0, 0, 0, 7, 0, 1, 4, 33, 0, 57, 0];
        from_slice::<Vec<i16>>(&data).unwrap_err();
    }
}
