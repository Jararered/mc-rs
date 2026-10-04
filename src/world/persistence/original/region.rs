//! Beta's McRegion file (`r.<x>.<z>.mcr`): up to 32×32 chunks in one file.
//!
//! The file is made of 4096-byte sectors. Sector 0 holds 1024 big-endian
//! offset entries, `(first sector << 8) | sector count`, with 0 for a chunk
//! that was never saved. Sector 1 holds each chunk's last-save time in
//! seconds. A chunk's record starts at its first sector: a `u32` length that
//! counts the compression byte, the byte itself (1 gzip, 2 zlib), then the
//! compressed NBT.
//!
//! This only moves compressed bytes in and out, so callers can compress and
//! decompress without holding the lock that guards the file.

use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const SECTOR_BYTES: u64 = 4096;
const SECTOR_INTS: usize = 1024;
/// Sectors taken by the offset and timestamp tables.
const HEADER_SECTORS: usize = 2;
/// A record's sector count is stored in one byte, so a chunk needing 256 or
/// more cannot be saved.
const MAX_SECTORS: usize = 256;

pub const COMPRESSION_GZIP: u8 = 1;
pub const COMPRESSION_ZLIB: u8 = 2;

/// An open region file and its allocation state.
pub struct RegionFile {
    file: File,
    offsets: [u32; SECTOR_INTS],
    /// Whether each sector of the file is unused.
    free: Vec<bool>,
}

fn slot(x: i32, z: i32) -> usize {
    (x & 31) as usize + (z & 31) as usize * 32
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.to_owned())
}

impl RegionFile {
    /// Open an existing region file, or with `create` make an empty one.
    /// Returns `None` when the file does not exist and `create` is false.
    pub fn open(path: &Path, create: bool) -> io::Result<Option<Self>> {
        let file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(create)
            .truncate(false)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        Self::from_file(file).map(Some)
    }

    fn from_file(mut file: File) -> io::Result<Self> {
        let mut length = file.metadata()?.len();
        if length < SECTOR_BYTES * HEADER_SECTORS as u64 {
            // New, or cut short before the tables were complete.
            file.set_len(SECTOR_BYTES * HEADER_SECTORS as u64)?;
            length = SECTOR_BYTES * HEADER_SECTORS as u64;
        }
        if length % SECTOR_BYTES != 0 {
            length = length.div_ceil(SECTOR_BYTES) * SECTOR_BYTES;
            file.set_len(length)?;
        }
        let sectors = (length / SECTOR_BYTES) as usize;
        let mut free = vec![true; sectors];
        free[..HEADER_SECTORS].fill(false);

        let mut header = vec![0; SECTOR_BYTES as usize * HEADER_SECTORS];
        file.seek(SeekFrom::Start(0))?;
        file.read_exact(&mut header)?;
        let mut offsets = [0; SECTOR_INTS];
        for index in 0..SECTOR_INTS {
            let at = index * 4;
            offsets[index] =
                u32::from_be_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]);
            let (start, count) = (
                (offsets[index] >> 8) as usize,
                (offsets[index] & 0xFF) as usize,
            );
            if offsets[index] != 0 && start >= HEADER_SECTORS && start + count <= sectors {
                free[start..start + count].fill(false);
            }
        }
        Ok(Self {
            file,
            offsets,
            free,
        })
    }

    /// The compression type and compressed payload of a chunk, or `None` if the
    /// chunk was never saved or its record is damaged.
    pub fn read(&mut self, x: i32, z: i32) -> io::Result<Option<(u8, Vec<u8>)>> {
        let offset = self.offsets[slot(x, z)];
        if offset == 0 {
            return Ok(None);
        }
        let (start, count) = ((offset >> 8) as usize, (offset & 0xFF) as usize);
        if start < HEADER_SECTORS || start + count > self.free.len() {
            return Ok(None);
        }
        self.file
            .seek(SeekFrom::Start(start as u64 * SECTOR_BYTES))?;
        let mut length = [0; 4];
        self.file.read_exact(&mut length)?;
        let length = u32::from_be_bytes(length) as usize;
        if length == 0 || length > count * SECTOR_BYTES as usize {
            return Err(invalid("region chunk length is out of range"));
        }
        let mut record = vec![0; length];
        self.file.read_exact(&mut record)?;
        let compression = record.remove(0);
        Ok(Some((compression, record)))
    }

    /// Store a zlib-compressed chunk, reusing its sectors when the new record
    /// needs the same number and otherwise moving it to the first free run, or
    /// the end of the file.
    pub fn write(&mut self, x: i32, z: i32, compressed: &[u8]) -> io::Result<()> {
        let index = slot(x, z);
        let needed = (compressed.len() + 5).div_ceil(SECTOR_BYTES as usize);
        if needed >= MAX_SECTORS {
            return Err(io::Error::other("chunk is too large for a region file"));
        }
        let offset = self.offsets[index];
        let (old_start, old_count) = ((offset >> 8) as usize, (offset & 0xFF) as usize);
        let in_file = offset != 0 && old_start + old_count <= self.free.len();

        let start = if in_file && old_count == needed {
            old_start
        } else {
            if in_file {
                self.free[old_start..old_start + old_count].fill(true);
            }
            let start = self
                .find_free_run(needed)
                .unwrap_or_else(|| self.free.len());
            if start + needed > self.free.len() {
                self.free.resize(start + needed, true);
                self.file.set_len((start + needed) as u64 * SECTOR_BYTES)?;
            }
            self.free[start..start + needed].fill(false);
            start
        };

        let mut record = Vec::with_capacity(compressed.len() + 5);
        record.extend_from_slice(&(compressed.len() as u32 + 1).to_be_bytes());
        record.push(COMPRESSION_ZLIB);
        record.extend_from_slice(compressed);
        self.file
            .seek(SeekFrom::Start(start as u64 * SECTOR_BYTES))?;
        self.file.write_all(&record)?;

        let entry = ((start as u32) << 8) | needed as u32;
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs() as u32);
        self.offsets[index] = entry;
        self.file.seek(SeekFrom::Start(index as u64 * 4))?;
        self.file.write_all(&entry.to_be_bytes())?;
        self.file
            .seek(SeekFrom::Start(SECTOR_BYTES + index as u64 * 4))?;
        self.file.write_all(&seconds.to_be_bytes())
    }

    fn find_free_run(&self, needed: usize) -> Option<usize> {
        let mut run = 0;
        for (sector, free) in self.free.iter().enumerate().skip(HEADER_SECTORS) {
            run = if *free { run + 1 } else { 0 };
            if run == needed {
                return Some(sector + 1 - needed);
            }
        }
        None
    }
}
