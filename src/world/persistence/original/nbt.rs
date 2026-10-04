//! Named Binary Tag, as read and written by Beta's `CompressedStreamTools`.
//!
//! Everything is big-endian. Strings are Java's modified UTF-8 (`writeUTF`):
//! a `u16` byte length, NUL encoded as `C0 80`, and supplementary characters
//! as two three-byte surrogates.

use std::io;

const END: u8 = 0;
const BYTE: u8 = 1;
const SHORT: u8 = 2;
const INT: u8 = 3;
const LONG: u8 = 4;
const FLOAT: u8 = 5;
const DOUBLE: u8 = 6;
const BYTE_ARRAY: u8 = 7;
const STRING: u8 = 8;
const LIST: u8 = 9;
const COMPOUND: u8 = 10;

/// Deeper nesting than any real save has; stops a corrupt file from recursing
/// until the stack overflows.
const MAX_DEPTH: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<u8>),
    String(String),
    List(Vec<Tag>),
    Compound(Compound),
}

impl Tag {
    fn id(&self) -> u8 {
        match self {
            Self::Byte(_) => BYTE,
            Self::Short(_) => SHORT,
            Self::Int(_) => INT,
            Self::Long(_) => LONG,
            Self::Float(_) => FLOAT,
            Self::Double(_) => DOUBLE,
            Self::ByteArray(_) => BYTE_ARRAY,
            Self::String(_) => STRING,
            Self::List(_) => LIST,
            Self::Compound(_) => COMPOUND,
        }
    }

    fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Byte(value) => Some(i64::from(*value)),
            Self::Short(value) => Some(i64::from(*value)),
            Self::Int(value) => Some(i64::from(*value)),
            Self::Long(value) => Some(*value),
            _ => None,
        }
    }

    fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Float(value) => Some(f64::from(*value)),
            Self::Double(value) => Some(*value),
            other => other.as_i64().map(|value| value as f64),
        }
    }
}

/// A compound tag. Keys keep their insertion order, which Beta's `HashMap`
/// does not, and nothing reading a save depends on.
///
/// The getters follow Beta's: a missing key reads as zero, an empty string or
/// an empty list. Integer getters also accept the other integer widths, so a
/// save that stored a count as a short instead of a byte still loads.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compound(Vec<(String, Tag)>);

impl Compound {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn put(&mut self, key: &str, tag: Tag) {
        match self.0.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = tag,
            None => self.0.push((key.to_owned(), tag)),
        }
    }

    pub fn put_byte(&mut self, key: &str, value: i8) {
        self.put(key, Tag::Byte(value));
    }

    pub fn put_bool(&mut self, key: &str, value: bool) {
        self.put(key, Tag::Byte(i8::from(value)));
    }

    pub fn put_short(&mut self, key: &str, value: i16) {
        self.put(key, Tag::Short(value));
    }

    pub fn put_int(&mut self, key: &str, value: i32) {
        self.put(key, Tag::Int(value));
    }

    pub fn put_long(&mut self, key: &str, value: i64) {
        self.put(key, Tag::Long(value));
    }

    pub fn put_float(&mut self, key: &str, value: f32) {
        self.put(key, Tag::Float(value));
    }

    pub fn put_string(&mut self, key: &str, value: &str) {
        self.put(key, Tag::String(value.to_owned()));
    }

    pub fn put_bytes(&mut self, key: &str, value: Vec<u8>) {
        self.put(key, Tag::ByteArray(value));
    }

    pub fn put_list(&mut self, key: &str, value: Vec<Tag>) {
        self.put(key, Tag::List(value));
    }

    pub fn put_compound(&mut self, key: &str, value: Self) {
        self.put(key, Tag::Compound(value));
    }

    pub fn get(&self, key: &str) -> Option<&Tag> {
        self.0
            .iter()
            .find_map(|(name, tag)| (name == key).then_some(tag))
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    fn integer(&self, key: &str) -> i64 {
        self.get(key).and_then(Tag::as_i64).unwrap_or(0)
    }

    pub fn byte(&self, key: &str) -> i8 {
        self.integer(key) as i8
    }

    pub fn boolean(&self, key: &str) -> bool {
        self.integer(key) != 0
    }

    pub fn short(&self, key: &str) -> i16 {
        self.integer(key) as i16
    }

    pub fn int(&self, key: &str) -> i32 {
        self.integer(key) as i32
    }

    pub fn long(&self, key: &str) -> i64 {
        self.integer(key)
    }

    pub fn string(&self, key: &str) -> &str {
        match self.get(key) {
            Some(Tag::String(value)) => value,
            _ => "",
        }
    }

    pub fn bytes(&self, key: &str) -> &[u8] {
        match self.get(key) {
            Some(Tag::ByteArray(value)) => value,
            _ => &[],
        }
    }

    pub fn list(&self, key: &str) -> &[Tag] {
        match self.get(key) {
            Some(Tag::List(value)) => value,
            _ => &[],
        }
    }

    pub fn compound(&self, key: &str) -> Option<&Self> {
        match self.get(key) {
            Some(Tag::Compound(value)) => Some(value),
            _ => None,
        }
    }

    /// A list of compounds, skipping any element of another type.
    pub fn compounds(&self, key: &str) -> impl Iterator<Item = &Self> {
        self.list(key).iter().filter_map(|tag| match tag {
            Tag::Compound(value) => Some(value),
            _ => None,
        })
    }

    /// A list of at least `N` numbers, as `Pos` and `Motion` are stored.
    pub fn numbers<const N: usize>(&self, key: &str) -> Option<[f64; N]> {
        let list = self.list(key);
        if list.len() < N {
            return None;
        }
        let mut values = [0.0; N];
        for (value, tag) in values.iter_mut().zip(list) {
            *value = tag.as_f64()?;
        }
        Some(values)
    }
}

pub fn doubles(values: &[f64]) -> Vec<Tag> {
    values.iter().map(|value| Tag::Double(*value)).collect()
}

pub fn floats(values: &[f32]) -> Vec<Tag> {
    values.iter().map(|value| Tag::Float(*value)).collect()
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    fn take(&mut self, count: usize) -> io::Result<&'a [u8]> {
        if count > self.bytes.len() {
            return Err(invalid("NBT data ends early"));
        }
        let (head, tail) = self.bytes.split_at(count);
        self.bytes = tail;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        let mut array = [0; N];
        array.copy_from_slice(self.take(N)?);
        Ok(array)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_be_bytes(self.array()?))
    }

    fn length(&mut self) -> io::Result<usize> {
        usize::try_from(self.i32()?).map_err(|_| invalid("negative NBT length"))
    }

    fn string(&mut self) -> io::Result<String> {
        let length = usize::from(u16::from_be_bytes(self.array()?));
        decode_modified_utf8(self.take(length)?)
    }

    fn payload(&mut self, id: u8, depth: usize) -> io::Result<Tag> {
        if depth > MAX_DEPTH {
            return Err(invalid("NBT nests too deeply"));
        }
        Ok(match id {
            BYTE => Tag::Byte(self.u8()? as i8),
            SHORT => Tag::Short(i16::from_be_bytes(self.array()?)),
            INT => Tag::Int(self.i32()?),
            LONG => Tag::Long(i64::from_be_bytes(self.array()?)),
            FLOAT => Tag::Float(f32::from_be_bytes(self.array()?)),
            DOUBLE => Tag::Double(f64::from_be_bytes(self.array()?)),
            BYTE_ARRAY => {
                let length = self.length()?;
                Tag::ByteArray(self.take(length)?.to_vec())
            }
            STRING => Tag::String(self.string()?),
            LIST => {
                let element = self.u8()?;
                let count = self.length()?;
                // A list of `End` holds nothing. Any other element takes at
                // least a byte, so the data bounds the allocation.
                if element == END {
                    return Ok(Tag::List(Vec::new()));
                }
                if count > self.bytes.len() {
                    return Err(invalid("NBT list longer than its data"));
                }
                let mut tags = Vec::with_capacity(count);
                for _ in 0..count {
                    tags.push(self.payload(element, depth + 1)?);
                }
                Tag::List(tags)
            }
            COMPOUND => Tag::Compound(self.compound(depth + 1)?),
            _ => return Err(invalid("unknown NBT tag type")),
        })
    }

    fn compound(&mut self, depth: usize) -> io::Result<Compound> {
        let mut compound = Compound::new();
        loop {
            let id = self.u8()?;
            if id == END {
                return Ok(compound);
            }
            let name = self.string()?;
            let tag = self.payload(id, depth)?;
            compound.put(&name, tag);
        }
    }
}

/// Parse an uncompressed NBT document whose root is a compound, like Beta's
/// `CompressedStreamTools.read`. The root's name is ignored.
pub fn read_root(bytes: &[u8]) -> io::Result<Compound> {
    let mut reader = Reader { bytes };
    if reader.u8()? != COMPOUND {
        return Err(invalid("NBT root is not a compound"));
    }
    reader.string()?;
    reader.compound(0)
}

fn write_string(out: &mut Vec<u8>, value: &str) {
    let encoded = encode_modified_utf8(value);
    // `writeUTF` rejects anything past 65535 bytes; cut at a character boundary
    // instead of failing a whole save over one long name.
    let mut length = encoded.len().min(usize::from(u16::MAX));
    while length > 0 && length < encoded.len() && (encoded[length] & 0xC0) == 0x80 {
        length -= 1;
    }
    out.extend_from_slice(&(length as u16).to_be_bytes());
    out.extend_from_slice(&encoded[..length]);
}

fn write_payload(out: &mut Vec<u8>, tag: &Tag) {
    match tag {
        Tag::Byte(value) => out.push(*value as u8),
        Tag::Short(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Int(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Long(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Float(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::Double(value) => out.extend_from_slice(&value.to_be_bytes()),
        Tag::ByteArray(value) => {
            out.extend_from_slice(&(value.len() as i32).to_be_bytes());
            out.extend_from_slice(value);
        }
        Tag::String(value) => write_string(out, value),
        Tag::List(tags) => {
            out.push(tags.first().map_or(BYTE, Tag::id));
            out.extend_from_slice(&(tags.len() as i32).to_be_bytes());
            for tag in tags {
                write_payload(out, tag);
            }
        }
        Tag::Compound(compound) => write_compound(out, compound),
    }
}

fn write_compound(out: &mut Vec<u8>, compound: &Compound) {
    for (name, tag) in &compound.0 {
        out.push(tag.id());
        write_string(out, name);
        write_payload(out, tag);
    }
    out.push(END);
}

/// Serialize `root` as an uncompressed document with an empty root name.
pub fn write_root(root: &Compound) -> Vec<u8> {
    let mut out = vec![COMPOUND];
    write_string(&mut out, "");
    write_compound(&mut out, root);
    out
}

fn encode_modified_utf8(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len());
    for unit in value.encode_utf16() {
        match unit {
            0x0001..=0x007F => out.push(unit as u8),
            0x0000 | 0x0080..=0x07FF => {
                out.push(0xC0 | (unit >> 6) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
            _ => {
                out.push(0xE0 | (unit >> 12) as u8);
                out.push(0x80 | ((unit >> 6) & 0x3F) as u8);
                out.push(0x80 | (unit & 0x3F) as u8);
            }
        }
    }
    out
}

fn decode_modified_utf8(bytes: &[u8]) -> io::Result<String> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let first = bytes[index];
        let continuation = |offset: usize| {
            bytes
                .get(index + offset)
                .filter(|byte| *byte & 0xC0 == 0x80)
                .map(|byte| u16::from(byte & 0x3F))
                .ok_or_else(|| invalid("malformed modified UTF-8"))
        };
        if first & 0x80 == 0 {
            units.push(u16::from(first));
            index += 1;
        } else if first & 0xE0 == 0xC0 {
            units.push((u16::from(first & 0x1F) << 6) | continuation(1)?);
            index += 2;
        } else if first & 0xF0 == 0xE0 {
            units
                .push((u16::from(first & 0x0F) << 12) | (continuation(1)? << 6) | continuation(2)?);
            index += 3;
        } else {
            return Err(invalid("malformed modified UTF-8"));
        }
    }
    Ok(String::from_utf16_lossy(&units))
}
