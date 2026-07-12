//! A sector-aligning `Read + Seek` adapter.
//!
//! On Windows, a raw volume handle (`\\.\C:`) only permits reads whose offset
//! and length are multiples of the sector size. The [`ntfs`] parser, however,
//! reads arbitrary byte ranges. This adapter bridges the two: it presents a
//! byte-granular `Read + Seek` interface while only ever issuing sector-aligned
//! reads to the underlying handle, buffering one aligned block at a time.
//!
//! It is transparent over any `Read + Seek` (including plain files and
//! in-memory cursors), which lets us unit-test it — and everything layered on
//! top of it — on non-Windows platforms.

use std::io::{self, Read, Seek, SeekFrom};

/// Wraps a `Read + Seek` source and services byte-granular reads through
/// sector-aligned block reads.
#[derive(Debug)]
pub struct AlignedReader<R> {
    inner: R,
    /// Sector size in bytes; every physical read is aligned to this.
    sector: u64,
    /// Logical cursor position exposed to callers.
    pos: u64,
    /// The currently buffered aligned block and the offset it starts at.
    block: Vec<u8>,
    block_start: u64,
    block_len: usize,
}

impl<R: Read + Seek> AlignedReader<R> {
    /// Create an adapter with the given sector size (must be a power of two,
    /// typically 512 or 4096).
    #[must_use]
    pub fn new(inner: R, sector: u64) -> Self {
        debug_assert!(
            sector.is_power_of_two(),
            "sector size must be a power of two"
        );
        Self {
            inner,
            sector,
            pos: 0,
            block: vec![0u8; sector as usize],
            block_start: 0,
            block_len: 0,
        }
    }

    /// Round `offset` down to the nearest sector boundary.
    fn align_down(&self, offset: u64) -> u64 {
        offset & !(self.sector - 1)
    }

    /// Ensure the block buffer contains the sector covering `self.pos`.
    fn fill_block_for_pos(&mut self) -> io::Result<()> {
        let block_start = self.align_down(self.pos);
        if self.block_len > 0 && block_start == self.block_start {
            return Ok(()); // already buffered
        }
        self.inner.seek(SeekFrom::Start(block_start))?;
        // Read exactly one sector; tolerate a short read at EOF.
        let mut filled = 0usize;
        let want = self.sector as usize;
        while filled < want {
            match self.inner.read(&mut self.block[filled..want]) {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
        self.block_start = block_start;
        self.block_len = filled;
        Ok(())
    }
}

impl<R: Read + Seek> Read for AlignedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.fill_block_for_pos()?;
        let block_offset = (self.pos - self.block_start) as usize;
        if block_offset >= self.block_len {
            return Ok(0); // at or past EOF
        }
        let available = self.block_len - block_offset;
        let n = available.min(buf.len());
        buf[..n].copy_from_slice(&self.block[block_offset..block_offset + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl<R: Read + Seek> Seek for AlignedReader<R> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let new_pos = match from {
            SeekFrom::Start(o) => o,
            SeekFrom::Current(d) => (self.pos as i64 + d) as u64,
            SeekFrom::End(d) => {
                let end = self.inner.seek(SeekFrom::End(0))?;
                (end as i64 + d) as u64
            }
        };
        self.pos = new_pos;
        Ok(self.pos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn aligned_reads_match_plain_reads() {
        let data: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let mut reader = AlignedReader::new(Cursor::new(data.clone()), 512);

        // Read an unaligned range spanning multiple sectors.
        reader.seek(SeekFrom::Start(500)).unwrap();
        let mut buf = vec![0u8; 1030];
        let mut got = 0;
        while got < buf.len() {
            let n = reader.read(&mut buf[got..]).unwrap();
            if n == 0 {
                break;
            }
            got += n;
        }
        assert_eq!(got, 1030);
        assert_eq!(&buf[..], &data[500..1530]);
    }

    #[test]
    fn short_read_at_eof() {
        let data = vec![7u8; 600];
        let mut reader = AlignedReader::new(Cursor::new(data), 512);
        reader.seek(SeekFrom::Start(590)).unwrap();
        let mut buf = vec![0u8; 100];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 10, "only 10 bytes remain before EOF");
        assert!(buf[..10].iter().all(|&b| b == 7));
    }
}
