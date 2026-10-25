//! Chunked authenticated encryption of a byte stream (OBR-DAT-046, D73): the container of backup
//! format 2.
//!
//! ```text
//! "OBOPBK2\n"                    8 bytes, magic
//! header_len                     u32 big endian
//! header                         that many bytes of JSON (cleartext, authenticated)
//! { ct_len u32 BE, ct }...       chunks: ChaCha20-Poly1305 of at most `chunk` plaintext bytes
//! ```
//!
//! Every chunk is sealed under one random per-stream data key. The nonce of chunk `i` is a random
//! 7 byte prefix, `i` as a u32 and a final flag byte (the STREAM construction of Hoang, Reyhanitabar,
//! Rogaway and Vizar): a chunk moved, repeated or dropped decrypts under the wrong nonce, and the
//! last chunk is the only one sealed with the flag, so cutting the file anywhere, even at a chunk
//! boundary, or appending to it, fails authentication. The associated data of every chunk is the
//! SHA-256 of the magic and the header, so a chunk cannot be moved to another stream and the
//! header cannot be edited. Compression happens before sealing (a backup is not interactive, so
//! the length side channel of compress-then-encrypt does not apply).
//!
//! Next to the cipher a hash chain runs over the chunk tags (`chain_i = SHA-256(chain_{i-1} ||
//! i || tag_i)`, `chain_0` over the header hash). It needs no key, so a file can be checked
//! for structure and compared with the head the manifest records without being able to read it.
//!
//! Memory is one chunk. Nothing here prints or returns a key or a plaintext in an error.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use sha2::{Digest, Sha256};
use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 8] = b"OBOPBK2\n";
/// The plaintext size of a chunk this build writes.
pub const CHUNK: usize = 64 * 1024;
const TAG: usize = 16;
const MAX_HEADER: usize = 64 * 1024;
const MAX_CHUNK: usize = 4 * 1024 * 1024;

/// Why a sealed stream did not open. The messages are fixed text: no key, no plaintext.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    /// Not a sealed stream, or a header that cannot be read.
    Malformed(&'static str),
    /// The stream ends before its final chunk.
    Truncated,
    /// Bytes follow the final chunk, or a chunk claims to be last and is not.
    Extended,
    /// A chunk does not authenticate: edited, reordered, from another stream, or the wrong key.
    Damaged(u32),
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamError::Malformed(w) => write!(f, "not a sealed backup stream: {w}"),
            StreamError::Truncated => write!(
                f,
                "the sealed stream ends before its last chunk: the file is truncated"
            ),
            StreamError::Extended => {
                write!(f, "data follows the last chunk: the file was extended")
            }
            StreamError::Damaged(i) => write!(
                f,
                "chunk {i} does not authenticate: the file was changed, rearranged, or the key is wrong"
            ),
        }
    }
}

impl std::error::Error for StreamError {}

impl From<StreamError> for io::Error {
    fn from(e: StreamError) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidData, e)
    }
}

/// The error of a failed read, if it is one of ours.
pub fn stream_error(e: &io::Error) -> Option<&StreamError> {
    e.get_ref().and_then(|i| i.downcast_ref::<StreamError>())
}

fn nonce(prefix: &[u8; 7], counter: u32, last: bool) -> Nonce {
    let mut n = [0u8; 12];
    n[..7].copy_from_slice(prefix);
    n[7..11].copy_from_slice(&counter.to_be_bytes());
    n[11] = u8::from(last);
    Nonce::from(n)
}

fn aad_of(header: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(MAGIC);
    h.update((header.len() as u32).to_be_bytes());
    h.update(header);
    h.finalize().into()
}

fn chain_start(aad: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"obop-chain/v1");
    h.update(aad);
    h.finalize().into()
}

fn chain_next(prev: &[u8; 32], i: u32, tag: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(prev);
    h.update(i.to_be_bytes());
    h.update(tag);
    h.finalize().into()
}

/// What a finished stream consists of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed {
    /// Number of chunks.
    pub chunks: u64,
    /// Hex of the head of the hash chain over the chunk tags.
    pub chain_head: String,
    /// Plaintext bytes sealed.
    pub plain_bytes: u64,
}

/// Seals what is written to it into `inner`.
pub struct SealWriter<W: Write> {
    inner: W,
    cipher: ChaCha20Poly1305,
    prefix: [u8; 7],
    aad: [u8; 32],
    chunk: usize,
    counter: u32,
    buf: Vec<u8>,
    chain: [u8; 32],
    plain: u64,
}

impl<W: Write> SealWriter<W> {
    /// Write the magic and `header` and get ready to seal. `chunk` is the plaintext chunk size
    /// (the header must record it, the reader needs it).
    pub fn new(
        mut inner: W,
        header: &[u8],
        data_key: &[u8; 32],
        prefix: [u8; 7],
        chunk: usize,
    ) -> io::Result<Self> {
        if header.len() > MAX_HEADER || !(16..=MAX_CHUNK).contains(&chunk) {
            return Err(StreamError::Malformed("header or chunk size out of range").into());
        }
        inner.write_all(MAGIC)?;
        inner.write_all(&(header.len() as u32).to_be_bytes())?;
        inner.write_all(header)?;
        let aad = aad_of(header);
        Ok(SealWriter {
            inner,
            cipher: ChaCha20Poly1305::new(data_key.into()),
            prefix,
            aad,
            chunk,
            counter: 0,
            buf: Vec::with_capacity(chunk + 1),
            chain: chain_start(&aad),
            plain: 0,
        })
    }

    fn emit(&mut self, plain: &[u8], last: bool) -> io::Result<()> {
        let ct = self
            .cipher
            .encrypt(
                &nonce(&self.prefix, self.counter, last),
                Payload {
                    msg: plain,
                    aad: &self.aad,
                },
            )
            .map_err(|_| io::Error::other("encryption failed"))?;
        self.inner.write_all(&(ct.len() as u32).to_be_bytes())?;
        self.inner.write_all(&ct)?;
        self.chain = chain_next(&self.chain, self.counter, &ct[ct.len() - TAG..]);
        self.plain += plain.len() as u64;
        self.counter = self
            .counter
            .checked_add(1)
            .ok_or_else(|| io::Error::other("a sealed stream holds at most 2^32 chunks"))?;
        Ok(())
    }

    /// Seal the rest as the final chunk and return the inner writer and what was written.
    pub fn finish(mut self) -> io::Result<(W, Sealed)> {
        let rest = std::mem::take(&mut self.buf);
        self.emit(&rest, true)?;
        self.inner.flush()?;
        Ok((
            self.inner,
            Sealed {
                chunks: u64::from(self.counter),
                chain_head: crate::hex(&self.chain),
                plain_bytes: self.plain,
            },
        ))
    }
}

impl<W: Write> Write for SealWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        // Keep at least one byte back: the last chunk is sealed with its own flag at `finish`.
        while self.buf.len() > self.chunk {
            let rest = self.buf.split_off(self.chunk);
            let full = std::mem::replace(&mut self.buf, rest);
            self.emit(&full, false)?;
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Read the magic and the header of a sealed stream. The header is cleartext: use it to find
/// the key, then open the rest with [`OpenReader::new`].
pub fn read_header<R: Read>(inner: &mut R) -> io::Result<Vec<u8>> {
    let mut magic = [0u8; 8];
    inner
        .read_exact(&mut magic)
        .map_err(|_| io::Error::from(StreamError::Malformed("too short")))?;
    if &magic != MAGIC {
        return Err(StreamError::Malformed("wrong magic").into());
    }
    let mut len = [0u8; 4];
    inner
        .read_exact(&mut len)
        .map_err(|_| io::Error::from(StreamError::Malformed("too short")))?;
    let len = u32::from_be_bytes(len) as usize;
    if len == 0 || len > MAX_HEADER {
        return Err(StreamError::Malformed("header length").into());
    }
    let mut header = vec![0u8; len];
    inner
        .read_exact(&mut header)
        .map_err(|_| io::Error::from(StreamError::Malformed("header cut short")))?;
    Ok(header)
}

/// Opens a sealed stream chunk by chunk and yields the plaintext.
pub struct OpenReader<R: Read> {
    inner: R,
    cipher: ChaCha20Poly1305,
    prefix: [u8; 7],
    aad: [u8; 32],
    chunk: usize,
    counter: u32,
    out: Vec<u8>,
    pos: usize,
    /// The length of the next chunk, read ahead to learn whether the current one is the last.
    ahead: Option<u32>,
    finished: bool,
    chain: [u8; 32],
}

impl<R: Read> OpenReader<R> {
    /// `header` is what [`read_header`] returned; the reader is positioned at the first chunk.
    pub fn new(
        inner: R,
        header: &[u8],
        data_key: &[u8; 32],
        prefix: [u8; 7],
        chunk: usize,
    ) -> io::Result<Self> {
        if !(16..=MAX_CHUNK).contains(&chunk) {
            return Err(StreamError::Malformed("chunk size out of range").into());
        }
        let aad = aad_of(header);
        Ok(OpenReader {
            inner,
            cipher: ChaCha20Poly1305::new(data_key.into()),
            prefix,
            aad,
            chunk,
            counter: 0,
            out: Vec::new(),
            pos: 0,
            ahead: None,
            finished: false,
            chain: chain_start(&aad),
        })
    }

    /// Hex of the chain head over the chunks read so far (the whole stream once read to the end).
    pub fn chain_head(&self) -> String {
        crate::hex(&self.chain)
    }

    pub fn chunks_read(&self) -> u64 {
        u64::from(self.counter)
    }

    fn read_len(&mut self) -> io::Result<Option<u32>> {
        let mut len = [0u8; 4];
        let mut got = 0;
        while got < 4 {
            let n = self.inner.read(&mut len[got..])?;
            if n == 0 {
                return if got == 0 {
                    Ok(None)
                } else {
                    Err(StreamError::Truncated.into())
                };
            }
            got += n;
        }
        Ok(Some(u32::from_be_bytes(len)))
    }

    /// Open the next chunk into `out`. Returns false at the end.
    fn next_chunk(&mut self) -> io::Result<bool> {
        if self.finished {
            return Ok(false);
        }
        let len = match self.ahead.take() {
            Some(l) => l,
            None => self.read_len()?.ok_or(StreamError::Truncated)?,
        } as usize;
        if !(TAG..=self.chunk + TAG).contains(&len) {
            return Err(StreamError::Damaged(self.counter).into());
        }
        let mut ct = vec![0u8; len];
        self.inner
            .read_exact(&mut ct)
            .map_err(|_| io::Error::from(StreamError::Truncated))?;
        // Is another chunk coming? Then this one is not the last.
        self.ahead = self.read_len()?;
        let last = self.ahead.is_none();
        let plain = self
            .cipher
            .decrypt(
                &nonce(&self.prefix, self.counter, last),
                Payload {
                    msg: &ct,
                    aad: &self.aad,
                },
            )
            .map_err(|_| {
                // A chunk that opens with the opposite flag is a stream cut or extended there.
                let other = self
                    .cipher
                    .decrypt(
                        &nonce(&self.prefix, self.counter, !last),
                        Payload {
                            msg: &ct,
                            aad: &self.aad,
                        },
                    )
                    .is_ok();
                io::Error::from(match (other, last) {
                    // It opens as a middle chunk, yet the file ends here: cut short.
                    (true, true) => StreamError::Truncated,
                    // It opens as the last chunk, yet more follows: extended.
                    (true, false) => StreamError::Extended,
                    (false, _) => StreamError::Damaged(self.counter),
                })
            })?;
        self.chain = chain_next(&self.chain, self.counter, &ct[ct.len() - TAG..]);
        self.counter += 1;
        self.finished = last;
        self.out = plain;
        self.pos = 0;
        Ok(true)
    }
}

impl<R: Read> Read for OpenReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.pos >= self.out.len() {
            if !self.next_chunk()? {
                return Ok(0);
            }
        }
        let n = buf.len().min(self.out.len() - self.pos);
        buf[..n].copy_from_slice(&self.out[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// Check the framing of a sealed stream and compute its chain without a key: every chunk has a
/// plausible length, the stream ends exactly after a chunk. Returns the number of chunks and the
/// chain head (hex). This detects a flipped bit only through the chain a manifest recorded; the
/// authenticity check needs the key.
pub fn scan<R: Read>(mut inner: R, chunk: usize) -> io::Result<(u64, String)> {
    let header = read_header(&mut inner)?;
    let aad = aad_of(&header);
    let mut chain = chain_start(&aad);
    let mut i: u32 = 0;
    loop {
        let mut len = [0u8; 4];
        let mut got = 0;
        while got < 4 {
            let n = inner.read(&mut len[got..])?;
            if n == 0 {
                if got == 0 && i > 0 {
                    return Ok((u64::from(i), crate::hex(&chain)));
                }
                return Err(StreamError::Truncated.into());
            }
            got += n;
        }
        let len = u32::from_be_bytes(len) as usize;
        if !(TAG..=chunk + TAG).contains(&len) {
            return Err(StreamError::Damaged(i).into());
        }
        let mut ct = vec![0u8; len];
        inner
            .read_exact(&mut ct)
            .map_err(|_| io::Error::from(StreamError::Truncated))?;
        chain = chain_next(&chain, i, &ct[len - TAG..]);
        i = i
            .checked_add(1)
            .ok_or(StreamError::Malformed("too many chunks"))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 32] = [7u8; 32];
    const PREFIX: [u8; 7] = [1, 2, 3, 4, 5, 6, 7];

    fn seal(data: &[u8], chunk: usize) -> (Vec<u8>, Sealed) {
        let mut w = SealWriter::new(Vec::new(), b"{\"h\":1}", &KEY, PREFIX, chunk).unwrap();
        // Uneven writes, to cross chunk boundaries at odd places.
        for part in data.chunks(37) {
            w.write_all(part).unwrap();
        }
        w.finish().unwrap()
    }

    fn open(bytes: &[u8], key: &[u8; 32], chunk: usize) -> io::Result<Vec<u8>> {
        let mut r = bytes;
        let header = read_header(&mut r)?;
        let mut o = OpenReader::new(r, &header, key, PREFIX, chunk)?;
        let mut out = Vec::new();
        o.read_to_end(&mut out)?;
        Ok(out)
    }

    fn data(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i * 31 % 251) as u8).collect()
    }

    #[test]
    fn streams_of_every_awkward_length_round_trip() {
        for n in [0, 1, 15, 16, 17, 31, 32, 33, 64, 1000] {
            let d = data(n);
            let (bytes, s) = seal(&d, 16);
            assert_eq!(open(&bytes, &KEY, 16).unwrap(), d, "{n} bytes");
            assert_eq!(s.plain_bytes, n as u64);
            assert_eq!(
                s.chunks,
                (n.max(1).div_ceil(16)) as u64,
                "{n}: the last chunk is never empty unless the stream is"
            );
        }
        // Real chunk size across several chunks.
        let d = data(CHUNK * 3 + 5);
        let (bytes, s) = seal(&d, CHUNK);
        assert_eq!(s.chunks, 4);
        assert_eq!(open(&bytes, &KEY, CHUNK).unwrap(), d);
    }

    #[test]
    fn the_stream_hides_the_plaintext_and_differs_with_every_nonce_prefix() {
        let d = b"canary-9f3a7c-secret".repeat(40);
        let (bytes, _) = seal(&d, 64);
        assert!(!bytes.windows(12).any(|w| w == b"canary-9f3a7"));
        let mut w = SealWriter::new(Vec::new(), b"{\"h\":1}", &KEY, [9u8; 7], 64).unwrap();
        w.write_all(&d).unwrap();
        assert_ne!(bytes, w.finish().unwrap().0);
    }

    #[test]
    fn the_wrong_key_opens_nothing() {
        let (bytes, _) = seal(&data(500), 16);
        let e = open(&bytes, &[8u8; 32], 16).unwrap_err();
        assert_eq!(stream_error(&e), Some(&StreamError::Damaged(0)));
        assert!(!e.to_string().contains("0707"));
    }

    #[test]
    fn one_hundred_single_bit_flips_anywhere_are_each_detected() {
        let d = data(700);
        let (bytes, _) = seal(&d, 64);
        let mut caught = 0;
        for i in 0..100usize {
            let mut b = bytes.clone();
            // Spread the flips over the header, the length prefixes and the ciphertexts.
            let at = (i * 97 + 3) % b.len();
            b[at] ^= 1 << (i % 8);
            let r = open(&b, &KEY, 64);
            if r.is_err() || r.unwrap() != d {
                caught += 1;
            }
        }
        assert_eq!(caught, 100);
    }

    #[test]
    fn truncation_at_any_point_even_at_a_chunk_boundary_is_detected() {
        let (bytes, _) = seal(&data(200), 64);
        for cut in 1..bytes.len() {
            let r = open(&bytes[..cut], &KEY, 64);
            assert!(r.is_err(), "cut at {cut} of {} opened", bytes.len());
        }
        // Cutting exactly after chunk 0 is the interesting one.
        let first_end = 8 + 4 + 7 + 4 + 64 + TAG;
        let e = open(&bytes[..first_end], &KEY, 64).unwrap_err();
        assert_eq!(stream_error(&e), Some(&StreamError::Truncated));
    }

    #[test]
    fn extending_reordering_and_repeating_chunks_is_detected() {
        let (bytes, _) = seal(&data(200), 64);
        // Appended bytes.
        let mut more = bytes.clone();
        more.extend_from_slice(&[0, 0, 0, 20]);
        more.extend_from_slice(&[1u8; 20]);
        assert!(open(&more, &KEY, 64).is_err());
        // The last chunk repeated.
        let frame = 4 + 64 + TAG;
        let head = 8 + 4 + 7;
        let mut swapped = bytes.clone();
        let (a, b) = (head, head + frame);
        let first: Vec<u8> = swapped[a..a + frame].to_vec();
        let second: Vec<u8> = swapped[b..b + frame].to_vec();
        swapped[a..a + frame].copy_from_slice(&second);
        swapped[b..b + frame].copy_from_slice(&first);
        assert!(open(&swapped, &KEY, 64).is_err());
        // Chunks of another stream under the same key but another header.
        let mut w = SealWriter::new(Vec::new(), b"{\"h\":2}", &KEY, PREFIX, 64).unwrap();
        w.write_all(&data(200)).unwrap();
        let other = w.finish().unwrap().0;
        let mut spliced = bytes.clone();
        spliced[head..].copy_from_slice(&other[head..]);
        assert!(open(&spliced, &KEY, 64).is_err());
    }

    #[test]
    fn the_hash_chain_needs_no_key_and_follows_every_chunk() {
        let (bytes, s) = seal(&data(300), 64);
        let (n, head) = scan(&bytes[..], 64).unwrap();
        assert_eq!((n, head.as_str()), (s.chunks, s.chain_head.as_str()));
        // The reader computes the same head.
        let mut r = &bytes[..];
        let header = read_header(&mut r).unwrap();
        let mut o = OpenReader::new(r, &header, &KEY, PREFIX, 64).unwrap();
        std::io::copy(&mut o, &mut std::io::sink()).unwrap();
        assert_eq!(o.chain_head(), s.chain_head);
        assert_eq!(o.chunks_read(), s.chunks);
        // A change anywhere in a chunk changes the head.
        let mut b = bytes.clone();
        let last = b.len() - 3;
        b[last] ^= 1;
        assert_ne!(scan(&b[..], 64).unwrap().1, s.chain_head);
        // A cut is a framing error.
        assert!(scan(&bytes[..bytes.len() - 5], 64).is_err());
        assert!(scan(&b"not a backup"[..], 64).is_err());
    }

    #[test]
    fn the_header_is_authenticated_not_secret() {
        let (bytes, _) = seal(&data(50), 16);
        assert!(bytes.windows(7).any(|w| w == b"{\"h\":1}"));
        let mut b = bytes.clone();
        let at = bytes.windows(7).position(|w| w == b"{\"h\":1}").unwrap() + 5;
        b[at] = b'9';
        assert!(open(&b, &KEY, 16).is_err());
        assert!(read_header(&mut &b"OBOPBK1\n\0\0\0\x01x"[..]).is_err());
        assert!(read_header(&mut &b"OBOP"[..]).is_err());
    }

    #[test]
    fn sizes_that_could_exhaust_memory_are_refused() {
        let mut huge = MAGIC.to_vec();
        huge.extend_from_slice(&u32::MAX.to_be_bytes());
        assert!(read_header(&mut &huge[..]).is_err());
        let (bytes, _) = seal(&data(100), 64);
        // A length prefix far above the chunk size is damage, not an allocation.
        let mut b = bytes.clone();
        let at = 8 + 4 + 7;
        b[at..at + 4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(open(&b, &KEY, 64).is_err());
        assert!(SealWriter::new(Vec::new(), b"x", &KEY, PREFIX, 8).is_err());
        assert!(SealWriter::new(Vec::new(), b"x", &KEY, PREFIX, MAX_CHUNK + 1).is_err());
    }
}
