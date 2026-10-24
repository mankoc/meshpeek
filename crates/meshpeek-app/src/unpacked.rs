//! Models unpacked from solid archives, kept in memory for the next reads.
//!
//! In a solid 7z block or a solid RAR the files are compressed as one stream: reading one means decoding every
//! file before it. Stepping through the models of such an archive would decode its start again for every step.
//! Instead, one pass over the stream keeps each model it meets (up to [`MAX_BYTES`] in all, the least recently
//! used going first), reads a few models past the one asked for, and lets other reads of the same stream wait for
//! it rather than start a pass of their own.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, LazyLock, Mutex, MutexGuard};
use std::time::SystemTime;

/// Memory for kept models, all archives together.
const MAX_BYTES: usize = 512 << 20;
/// Models read past the last one asked for (the next steps through the archive)...
const READ_AHEAD: usize = 8;
/// ...as long as this much has not been decoded since (other files included).
const READ_AHEAD_BYTES: u64 = 256 << 20;
/// Passes over a stream one read may need: a model gone again before its reader took it starts another.
const MAX_PASSES: usize = 3;

/// One solid stream: the archive as it is on disk now (changed files are new streams) and, in a 7z, the block.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Stream {
    archive: PathBuf,
    modified: Option<SystemTime>,
    len: u64,
    block: usize,
}

impl Stream {
    pub fn new(archive: &Path, block: usize) -> Self {
        let meta = std::fs::metadata(archive).ok();
        Self {
            archive: archive.to_path_buf(),
            modified: meta.as_ref().and_then(|m| m.modified().ok()),
            len: meta.map_or(0, |m| m.len()),
            block,
        }
    }
}

/// What a decoder reports for each file it passes, in stream order: its name (with `/`), its size, and its bytes
/// when it is a model. Returns whether to go on.
pub type Visit<'a> = dyn FnMut(String, u64, Option<Vec<u8>>) -> bool + 'a;

/// Decodes a stream from its start, calling the visitor for each file until it says stop.
pub type Decode = dyn Fn(&mut Visit<'_>) -> Result<(), String> + Send + Sync;

/// The bytes of the model `entry` of `stream`: kept from an earlier pass, or from a pass of `decode` (on a thread
/// of its own, so that it can read ahead after handing this one over).
pub fn read(stream: Stream, entry: &str, decode: Arc<Decode>) -> Result<Vec<u8>, String> {
    let mut state = lock();
    let mut passes = 0;
    loop {
        if let Some(data) = state.take(&stream, entry) {
            drop(state);
            return Ok(Vec::clone(&data)); // copied outside the lock
        }
        if let Some(error) = state.failed.remove(&(stream.clone(), entry.to_string())) {
            return Err(error);
        }
        match state.passes.get_mut(&stream) {
            Some(pass) => {
                // under way: it keeps the entry if it has not passed it yet, otherwise start again once it ends
                if !pass.passed.contains(entry) {
                    pass.wanted.insert(entry.to_string());
                }
            }
            None if passes == MAX_PASSES => return Err(format!("{entry}: not enough memory to unpack it")),
            None => {
                passes += 1;
                let pass = Pass { wanted: HashSet::from([entry.to_string()]), ..Pass::default() };
                state.passes.insert(stream.clone(), pass);
                let (passing, decode) = (stream.clone(), decode.clone());
                let spawned = std::thread::Builder::new().name("unpack".into()).spawn(move || {
                    let result = decode(&mut |name, size, data| visit(&passing, name, size, data));
                    finish(&passing, result);
                });
                if let Err(e) = spawned {
                    state.passes.remove(&stream);
                    return Err(format!("could not start unpacking: {e}"));
                }
            }
        }
        state = CHANGED.wait(state).unwrap();
    }
}

/// Kept models and passes under way, shared by every loader thread.
#[derive(Default)]
struct State {
    /// The kept models, least recently used first.
    kept: Vec<Kept>,
    /// Their size in all.
    bytes: usize,
    passes: HashMap<Stream, Pass>,
    /// Entries a pass ended without (a decoding error, or not in the stream), with the reason, for their readers.
    failed: HashMap<(Stream, String), String>,
}

struct Kept {
    stream: Stream,
    entry: String,
    data: Arc<Vec<u8>>,
}

/// A pass over a stream.
#[derive(Default)]
struct Pass {
    /// Entries readers are waiting for, not reached yet.
    wanted: HashSet<String>,
    /// Entries passed.
    passed: HashSet<String>,
    /// Models kept and bytes decoded since the last wanted entry.
    ahead: (usize, u64),
}

static STATE: LazyLock<Mutex<State>> = LazyLock::new(Mutex::default);
/// Signalled whenever a pass keeps a model or ends.
static CHANGED: Condvar = Condvar::new();

fn lock() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl State {
    /// The kept bytes of `entry`, now the most recently used.
    fn take(&mut self, stream: &Stream, entry: &str) -> Option<Arc<Vec<u8>>> {
        let at = self.kept.iter().position(|k| k.stream == *stream && k.entry == entry)?;
        let kept = self.kept.remove(at);
        let data = kept.data.clone();
        self.kept.push(kept);
        Some(data)
    }

    /// Keep `data`. A wanted model makes room by forgetting the least recently used ones; one read ahead is only
    /// kept if it fits. Returns whether it is kept.
    fn keep(&mut self, stream: &Stream, entry: String, data: Vec<u8>, wanted: bool) -> bool {
        if self.kept.iter().any(|k| k.stream == *stream && k.entry == entry) {
            return true; // from an earlier pass
        }
        if !wanted && self.bytes + data.len() > MAX_BYTES {
            return false;
        }
        self.bytes += data.len();
        self.kept.push(Kept { stream: stream.clone(), entry, data: Arc::new(data) });
        while self.bytes > MAX_BYTES && self.kept.len() > 1 {
            let old = self.kept.remove(0);
            self.bytes -= old.data.len();
        }
        true
    }
}

/// A pass met the file `name`: keep it if it is a model, wake its readers, and say whether to go on (while
/// something is wanted, then for a few models more).
fn visit(stream: &Stream, name: String, size: u64, data: Option<Vec<u8>>) -> bool {
    let mut state = lock();
    let Some(pass) = state.passes.get_mut(stream) else { return false };
    pass.passed.insert(name.clone());
    let wanted = pass.wanted.remove(&name);
    if wanted {
        pass.ahead = (0, 0);
    } else {
        pass.ahead.1 += size;
    }
    let reading_ahead = !wanted;
    let still_wanted = !pass.wanted.is_empty();
    let mut room = true;
    if let Some(data) = data {
        room = state.keep(stream, name, data, wanted);
        if room && reading_ahead {
            state.passes.get_mut(stream).expect("pass under way").ahead.0 += 1;
        }
    }
    CHANGED.notify_all();
    let (models, bytes) = state.passes[stream].ahead;
    still_wanted || (room && models < READ_AHEAD && bytes < READ_AHEAD_BYTES)
}

/// A pass ended: entries still wanted were not found (or the stream could not be decoded).
fn finish(stream: &Stream, result: Result<(), String>) {
    let mut state = lock();
    if let Some(pass) = state.passes.remove(stream) {
        for entry in pass.wanted {
            let reason = match &result {
                Err(e) => e.clone(),
                Ok(()) => format!("{entry} not found in the archive"),
            };
            state.failed.insert((stream.clone(), entry), reason);
        }
    }
    CHANGED.notify_all();
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A stream of `files` (name, bytes; models end in `.stl`) that counts its passes.
    fn fake(files: Vec<(&'static str, Vec<u8>)>, passes: Arc<AtomicUsize>) -> Arc<Decode> {
        Arc::new(move |visit: &mut Visit<'_>| {
            passes.fetch_add(1, Ordering::SeqCst);
            for (name, bytes) in &files {
                let model = name.ends_with(".stl").then(|| bytes.clone());
                if !visit(name.to_string(), bytes.len() as u64, model) {
                    break;
                }
            }
            Ok(())
        })
    }

    #[test]
    fn one_pass_serves_the_models_before_and_after() {
        let dir = std::env::temp_dir().join("meshpeek-unpacked-test");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("one-pass.7z");
        std::fs::write(&archive, "x").unwrap();
        let stream = Stream::new(&archive, 0);
        let passes = Arc::new(AtomicUsize::new(0));
        let files = vec![("a.stl", vec![1]), ("notes.txt", vec![9; 3]), ("b.stl", vec![2]), ("c.stl", vec![3, 3])];
        let decode = fake(files, passes.clone());

        assert_eq!(read(stream.clone(), "b.stl", decode.clone()).unwrap(), [2]);
        // the one before was passed and the one after read ahead (or still being read): no other pass
        assert_eq!(read(stream.clone(), "a.stl", decode.clone()).unwrap(), [1]);
        assert_eq!(read(stream.clone(), "c.stl", decode.clone()).unwrap(), [3, 3]);
        assert_eq!(passes.load(Ordering::SeqCst), 1);

        while lock().passes.contains_key(&stream) {} // the first pass has ended
        let missing = read(stream.clone(), "d.stl", decode.clone()).unwrap_err();
        assert!(missing.contains("not found"), "{missing}");
        assert_eq!(passes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn a_decoding_error_reaches_the_reader() {
        let dir = std::env::temp_dir().join("meshpeek-unpacked-test");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("broken.7z");
        std::fs::write(&archive, "x").unwrap();
        let decode: Arc<Decode> = Arc::new(|_: &mut Visit<'_>| Err("data error".to_string()));
        assert_eq!(read(Stream::new(&archive, 0), "a.stl", decode).unwrap_err(), "data error");
    }

    #[test]
    fn readers_waiting_together_share_one_pass() {
        let dir = std::env::temp_dir().join("meshpeek-unpacked-test");
        std::fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("shared.7z");
        std::fs::write(&archive, "x").unwrap();
        let stream = Stream::new(&archive, 0);
        let passes = Arc::new(AtomicUsize::new(0));
        let names = ["m0.stl", "m1.stl", "m2.stl", "m3.stl", "m4.stl", "m5.stl"];
        let files: Vec<(&'static str, Vec<u8>)> = names.iter().map(|n| (*n, n.as_bytes().to_vec())).collect();
        let decode = fake(files, passes.clone());
        let readers: Vec<_> = names
            .into_iter()
            .rev()
            .map(|name| {
                let (stream, decode) = (stream.clone(), decode.clone());
                std::thread::spawn(move || read(stream, name, decode).unwrap() == name.as_bytes())
            })
            .collect();
        assert!(readers.into_iter().all(|r| r.join().unwrap()));
        // the readers that came while a pass was under way joined it; at most one pass per reader in the worst case
        assert!(passes.load(Ordering::SeqCst) <= names.len());
    }
}
