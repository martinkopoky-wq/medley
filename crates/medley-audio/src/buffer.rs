//! A growing, seekable byte stream backed by an unlinked temp file.
//!
//! The downloader appends while the decoder reads. Reads past the downloaded
//! end block until more data arrives, so symphonia can decode a track that is
//! still downloading. Backing the bytes with a file instead of a `Vec` keeps a
//! two-hour DJ mix out of resident memory — the kernel page cache handles it.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const READ_STALL: Duration = Duration::from_secs(30);

struct State {
    len: u64,
    total: Option<u64>,
    ready: bool,
    done: bool,
    error: Option<String>,
    cancelled: bool,
}

struct Shared {
    file: File,
    state: Mutex<State>,
    cond: Condvar,
}

#[derive(Clone)]
pub struct StreamBuffer {
    shared: Arc<Shared>,
}

impl StreamBuffer {
    /// Create a buffer in `dir`. The file is unlinked right away on Unix, so it
    /// disappears automatically when the last handle closes.
    pub fn new(dir: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("stream-{:016x}.part", rand_u64()));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path)?;
        #[cfg(unix)]
        std::fs::remove_file(&path)?;
        #[cfg(not(unix))]
        PENDING_DELETE.lock().expect("pending lock").push(path);
        Ok(Self {
            shared: Arc::new(Shared {
                file,
                state: Mutex::new(State {
                    len: 0,
                    total: None,
                    ready: false,
                    done: false,
                    error: None,
                    cancelled: false,
                }),
                cond: Condvar::new(),
            }),
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.shared.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn push(&self, bytes: &[u8]) -> io::Result<()> {
        let offset = self.lock().len;
        write_at(&self.shared.file, bytes, offset)?;
        let mut st = self.lock();
        st.len += bytes.len() as u64;
        st.ready = true;
        drop(st);
        self.shared.cond.notify_all();
        Ok(())
    }

    /// Response headers arrived; `total` is the Content-Length if known.
    pub fn set_ready(&self, total: Option<u64>) {
        let mut st = self.lock();
        st.total = total;
        st.ready = true;
        drop(st);
        self.shared.cond.notify_all();
    }

    pub fn finish(&self) {
        let mut st = self.lock();
        st.done = true;
        st.ready = true;
        st.total = Some(st.len);
        drop(st);
        self.shared.cond.notify_all();
    }

    pub fn fail(&self, message: String) {
        let mut st = self.lock();
        st.error = Some(message);
        st.done = true;
        st.ready = true;
        drop(st);
        self.shared.cond.notify_all();
    }

    pub fn cancel(&self) {
        self.lock().cancelled = true;
        self.shared.cond.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Block until headers (or an error) arrive. Returns the total length if known.
    pub fn wait_ready(&self, timeout: Duration) -> Result<Option<u64>, String> {
        let st = self.lock();
        let (st, _) = self
            .shared
            .cond
            .wait_timeout_while(st, timeout, |s| !s.ready && !s.cancelled)
            .unwrap_or_else(|p| p.into_inner());
        if let Some(e) = &st.error {
            return Err(e.clone());
        }
        if st.cancelled {
            return Err("cancelled".into());
        }
        if !st.ready {
            return Err("the stream didn't start in time".into());
        }
        Ok(st.total)
    }

    pub fn reader(&self) -> StreamReader {
        StreamReader { buf: self.clone(), pos: 0 }
    }
}

pub struct StreamReader {
    buf: StreamBuffer,
    pos: u64,
}

impl Read for StreamReader {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut st = self.buf.lock();
        loop {
            if st.cancelled {
                return Err(io::Error::other("stream cancelled"));
            }
            if self.pos < st.len {
                let n = ((st.len - self.pos) as usize).min(out.len());
                drop(st);
                let read = read_at(&self.buf.shared.file, &mut out[..n], self.pos)?;
                self.pos += read as u64;
                return Ok(read);
            }
            if st.done {
                return match &st.error {
                    Some(e) => Err(io::Error::other(e.clone())),
                    None => Ok(0),
                };
            }
            let before = st.len;
            let (guard, timeout) = self.buf.shared.cond.wait_timeout(st, READ_STALL).unwrap_or_else(|p| p.into_inner());
            st = guard;
            if timeout.timed_out() && st.len == before && !st.done {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "network stalled"));
            }
        }
    }
}

impl Seek for StreamReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(n) => n as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => {
                let mut st = self.buf.lock();
                while st.total.is_none() && !st.done && !st.cancelled {
                    st = self.buf.shared.cond.wait(st).unwrap_or_else(|p| p.into_inner());
                }
                st.total.unwrap_or(st.len) as i128 + d as i128
            }
        };
        if target < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        self.pos = target as u64;
        Ok(self.pos)
    }
}

#[cfg(unix)]
fn write_at(file: &File, buf: &[u8], offset: u64) -> io::Result<()> {
    std::os::unix::fs::FileExt::write_all_at(file, buf, offset)
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(windows)]
fn write_at(file: &File, mut buf: &[u8], mut offset: u64) -> io::Result<()> {
    while !buf.is_empty() {
        let n = std::os::windows::fs::FileExt::seek_write(file, buf, offset)?;
        buf = &buf[n..];
        offset += n as u64;
    }
    Ok(())
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

#[cfg(not(unix))]
static PENDING_DELETE: Mutex<Vec<std::path::PathBuf>> = Mutex::new(Vec::new());

/// Remove leftover stream files on platforms that can't unlink open files.
pub fn cleanup(dir: &Path) {
    #[cfg(not(unix))]
    {
        PENDING_DELETE.lock().expect("pending lock").retain(|p| std::fs::remove_file(p).is_err());
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            if e.path().extension().is_some_and(|x| x == "part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

fn rand_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reader_blocks_until_data_and_seeks() {
        let dir = std::env::temp_dir().join("medley-buffer-test");
        let buf = StreamBuffer::new(&dir).unwrap();
        let mut reader = buf.reader();
        let writer = buf.clone();
        let t = std::thread::spawn(move || {
            writer.set_ready(Some(10));
            writer.push(b"hello").unwrap();
            std::thread::sleep(Duration::from_millis(50));
            writer.push(b"world").unwrap();
            writer.finish();
        });
        let mut all = Vec::new();
        reader.read_to_end(&mut all).unwrap();
        t.join().unwrap();
        assert_eq!(all, b"helloworld");
        assert_eq!(reader.seek(SeekFrom::End(-5)).unwrap(), 5);
        let mut tail = String::new();
        reader.read_to_string(&mut tail).unwrap();
        assert_eq!(tail, "world");
    }

    #[test]
    fn errors_surface_to_reader() {
        let dir = std::env::temp_dir().join("medley-buffer-test");
        let buf = StreamBuffer::new(&dir).unwrap();
        buf.fail("boom".into());
        let mut out = [0u8; 4];
        assert!(buf.reader().read(&mut out).is_err());
        assert_eq!(buf.wait_ready(Duration::from_millis(1)), Err("boom".into()));
    }
}
