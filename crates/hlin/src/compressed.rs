//! Compressing what the shell serves, once per file (HLIN-T-0086,
//! HLIN-T-0091).
//!
//! Module assets under `/m/` and the shell's own frontend are sent gzip or
//! brotli where the browser accepts it. What is compressed is kept, keyed by
//! a SHA-256 of the uncompressed bytes and the encoding, so the second
//! request for a file, from anybody, costs a hash and a copy of an `Arc`
//! rather than a compression. Keyed by content rather than by path or `ETag`
//! because content is the only thing that cannot be wrong: two platforms
//! serving the same bytes share one entry, a file that changes under the
//! same name and validator is compressed afresh, and nothing a platform
//! declares can make the shell hand one module's file to another's frame.
//! A cryptographic hash for the same reason: a module's author must not be
//! able to make a file that collides with somebody else's.
//!
//! **Twice, in fact.** The first request for a file is answered at a quality
//! quick enough to be on the way to a browser (brotli 4, gzip's default), and
//! the result is kept. In the background, one at a time, the file is then
//! compressed again at brotli's best (11), which is two seconds of CPU for a
//! frontend's 1.7 MB wasm and a fifth fewer bytes than quality 4, and that
//! replaces what was kept. The shell's own frontend is compressed at its best
//! when the shell starts ([`Compressed::precompress`]), so nobody waits for
//! it. Gzip, which every browser that matters prefers brotli to, is kept at
//! its default: level 9 saves under one per cent of it.
//!
//! **Bounded by bytes.** Everything kept counts against
//! `[compression] cache_bytes` (64 MiB by default: all twenty of the demo's
//! modules and the frontend, both encodings, are about 5 MB), and the entry
//! used longest ago goes first. A file bigger than the whole budget is sent
//! as it is: it could not be kept, and compressing it on every request is
//! what this exists to stop. `cache_bytes = 0` turns compression off.
//!
//! **What is never compressed.** Anything but a `200`; anything already
//! encoded, which is how a platform that compresses its own files is taken
//! at its word; images, which already are; event streams and gRPC; anything
//! under a kilobyte, where the headers cost more than the saving; and
//! anything whose length is not known before it is read, which is a stream.
//! Never `/p/`, where a module's answer may be streamed and every piece must
//! reach it as it arrives: that route is not under this layer at all.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::{Body, HttpBody};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use sha2::Digest;

const KIB: usize = 1024;
const MIB: usize = 1024 * KIB;

/// Nothing smaller is compressed.
const SMALLEST: u64 = KIB as u64;

/// How many files may be waiting to be compressed at their best. Each holds
/// its uncompressed bytes until its turn, so the queue is bounded too; a file
/// that finds it full keeps its quick compression until it is next asked for.
const WAITING: usize = 64;

/// The shell-wide `[compression]` table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CompressionConfig {
    /// How many bytes of compressed files the shell keeps. `0` turns
    /// compression off.
    pub cache_bytes: usize,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            cache_bytes: 64 * MIB,
        }
    }
}

impl CompressionConfig {
    /// Whether this can be run with: zero, or at least a kilobyte, since a
    /// few bytes is a typo for a few kilobytes far more often than a choice.
    pub fn check(&self) -> Result<(), String> {
        if self.cache_bytes != 0 && self.cache_bytes < KIB {
            return Err(format!(
                "compression.cache_bytes is {}, under the floor of {KIB}; it is counted \
                 in bytes, and 0 turns compression off",
                self.cache_bytes
            ));
        }
        Ok(())
    }
}

/// An encoding the shell compresses to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// `br`.
    Brotli,
    /// `gzip`.
    Gzip,
}

impl Encoding {
    fn token(self) -> &'static str {
        match self {
            Self::Brotli => "br",
            Self::Gzip => "gzip",
        }
    }

    /// Compressed quickly enough to be on the way to a browser.
    fn quick(self, body: &[u8]) -> Vec<u8> {
        match self {
            Self::Brotli => brotli(body, 4),
            Self::Gzip => gzip(body),
        }
    }

    /// Whether [`Encoding::best`] does better than [`Encoding::quick`].
    fn improves(self) -> bool {
        self == Self::Brotli
    }

    /// Compressed as small as it goes, for keeping; `None` where that is
    /// what [`Encoding::quick`] already did.
    fn best(self, body: &[u8]) -> Option<Vec<u8>> {
        match self {
            Self::Brotli => Some(brotli(body, 11)),
            Self::Gzip => None,
        }
    }

    /// Which of these the browser would rather have, from its
    /// `Accept-Encoding`: the highest weight, brotli on a tie, and neither
    /// where it weighs them at zero or does not name them.
    pub fn accepted(headers: &HeaderMap) -> Option<Self> {
        let mut best: Option<(Self, f32)> = None;
        for value in headers.get_all(header::ACCEPT_ENCODING) {
            let Ok(value) = value.to_str() else { continue };
            for offer in value.split(',') {
                let mut parts = offer.split(';');
                let name = parts.next().unwrap_or("").trim();
                let encoding = if name.eq_ignore_ascii_case("br") {
                    Self::Brotli
                } else if name.eq_ignore_ascii_case("gzip") {
                    Self::Gzip
                } else {
                    continue;
                };
                let weight = parts
                    .find_map(|param| {
                        let (key, value) = param.split_once('=')?;
                        key.trim()
                            .eq_ignore_ascii_case("q")
                            .then(|| value.trim().parse::<f32>().ok())?
                    })
                    .unwrap_or(1.0);
                if weight <= 0.0 {
                    continue;
                }
                let better = match best {
                    None => true,
                    Some((chosen, chosen_weight)) => {
                        weight > chosen_weight
                            || (weight == chosen_weight
                                && encoding == Self::Brotli
                                && chosen != Self::Brotli)
                    }
                };
                if better {
                    best = Some((encoding, weight));
                }
            }
        }
        best.map(|(encoding, _)| encoding)
    }
}

fn brotli(body: &[u8], quality: i32) -> Vec<u8> {
    let params = brotli::enc::BrotliEncoderParams {
        quality,
        size_hint: body.len(),
        ..Default::default()
    };
    let mut out = Vec::with_capacity(body.len() / 3);
    brotli::BrotliCompress(&mut &body[..], &mut out, &params)
        .expect("compressing into memory cannot fail");
    out
}

fn gzip(body: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(
        Vec::with_capacity(body.len() / 3),
        flate2::Compression::default(),
    );
    encoder
        .write_all(body)
        .expect("compressing into memory cannot fail");
    encoder
        .finish()
        .expect("compressing into memory cannot fail")
}

type Key = ([u8; 32], Encoding);

fn key(body: &[u8], encoding: Encoding) -> Key {
    (sha2::Sha256::digest(body).into(), encoding)
}

struct Entry {
    body: Bytes,
    /// Compressed at the encoding's best, so never to be done again.
    best: bool,
    /// When it was last used, on [`Kept::clock`].
    used: u64,
}

#[derive(Default)]
struct Kept {
    entries: HashMap<Key, Entry>,
    /// The compressed bytes held in `entries`.
    bytes: usize,
    clock: u64,
    /// Waiting for, or having, their best compression.
    waiting: HashSet<Key>,
}

/// What the shell has compressed, shared by every route it compresses on.
pub struct Compressed {
    budget: usize,
    kept: Mutex<Kept>,
    /// One compression at its best at a time: these take seconds, and a cold
    /// surface asks for dozens of files at once.
    best: tokio::sync::Semaphore,
}

impl Default for Compressed {
    fn default() -> Self {
        Self::new(&CompressionConfig::default())
    }
}

/// How many bytes are kept, and in how many files, for the log and the
/// tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Held {
    /// Files and encodings kept.
    pub entries: usize,
    /// Compressed bytes kept.
    pub bytes: usize,
    /// Of those entries, how many are at their encoding's best.
    pub best: usize,
}

impl Compressed {
    /// An empty cache within `config`'s budget.
    pub fn new(config: &CompressionConfig) -> Self {
        Self {
            budget: config.cache_bytes,
            kept: Mutex::default(),
            best: tokio::sync::Semaphore::new(1),
        }
    }

    /// What is kept now.
    pub fn held(&self) -> Held {
        let kept = self.kept.lock().expect("never poisoned");
        Held {
            entries: kept.entries.len(),
            bytes: kept.bytes,
            best: kept.entries.values().filter(|entry| entry.best).count(),
        }
    }

    fn get(&self, key: &Key) -> Option<(Bytes, bool)> {
        let mut kept = self.kept.lock().expect("never poisoned");
        kept.clock += 1;
        let now = kept.clock;
        let entry = kept.entries.get_mut(key)?;
        entry.used = now;
        Some((entry.body.clone(), entry.best))
    }

    /// Keep `body` under `key`, making room by forgetting what was used
    /// longest ago. A quick compression never replaces a best one.
    fn keep(&self, key: Key, body: Bytes, best: bool) {
        if body.len() > self.budget {
            return;
        }
        let mut kept = self.kept.lock().expect("never poisoned");
        kept.clock += 1;
        let used = kept.clock;
        if let Some(old) = kept.entries.get(&key) {
            if old.best && !best {
                return;
            }
            let old = old.body.len();
            kept.bytes -= old;
        }
        kept.bytes += body.len();
        kept.entries.insert(key, Entry { body, best, used });

        while kept.bytes > self.budget {
            let Some(oldest) = kept
                .entries
                .iter()
                .filter(|(other, _)| **other != key)
                .min_by_key(|(_, entry)| entry.used)
                .map(|(other, _)| *other)
            else {
                break;
            };
            if let Some(gone) = kept.entries.remove(&oldest) {
                kept.bytes -= gone.body.len();
            }
        }
    }

    /// `body` compressed as `encoding`: kept if it was, compressed quickly
    /// and kept if not, with its best compression queued behind it.
    async fn compressed(self: &Arc<Self>, body: Bytes, encoding: Encoding) -> Bytes {
        let hashed = body.clone();
        let key = tokio::task::spawn_blocking(move || key(&hashed, encoding))
            .await
            .expect("hashing does not panic");
        if let Some((compressed, best)) = self.get(&key) {
            if !best {
                self.improve(key, body);
            }
            return compressed;
        }

        let raw = body.clone();
        let compressed = Bytes::from(
            tokio::task::spawn_blocking(move || encoding.quick(&raw))
                .await
                .expect("compressing does not panic"),
        );
        self.keep(key, compressed.clone(), !encoding.improves());
        self.improve(key, body);
        compressed
    }

    /// Queue `body` to be compressed at its best, unless it is already
    /// waiting, the queue is full, or its encoding has nothing better.
    fn improve(self: &Arc<Self>, key: Key, body: Bytes) {
        if !key.1.improves() {
            return;
        }
        {
            let mut kept = self.kept.lock().expect("never poisoned");
            if kept.waiting.len() >= WAITING || !kept.waiting.insert(key) {
                return;
            }
        }
        let cache = self.clone();
        tokio::spawn(async move {
            cache.at_best(key, body).await;
            cache
                .kept
                .lock()
                .expect("never poisoned")
                .waiting
                .remove(&key);
        });
    }

    async fn at_best(&self, key: Key, body: Bytes) {
        let Ok(_turn) = self.best.acquire().await else {
            return;
        };
        // Something forgotten while it waited is not brought back: whatever
        // pushed it out was used more recently.
        let quick = match self.get(&key) {
            Some((quick, false)) => quick,
            _ => return,
        };
        let started = std::time::Instant::now();
        let raw = body.clone();
        let Ok(Some(best)) = tokio::task::spawn_blocking(move || key.1.best(&raw)).await else {
            return;
        };
        let ms = started.elapsed().as_millis() as u64;
        let (bytes, quick_bytes, best_bytes) = (body.len(), quick.len(), best.len());
        // Brotli's best is not always smaller, on a small and repetitive
        // file; whichever is, is kept as final.
        let best = if best.len() < quick.len() {
            Bytes::from(best)
        } else {
            quick
        };
        self.keep(key, best, true);
        let held = self.held();
        tracing::debug!(
            encoding = key.1.token(),
            bytes,
            quick = quick_bytes,
            best = best_bytes,
            ms,
            kept_bytes = held.bytes,
            kept_files = held.entries,
            "compressed a file at its best"
        );
    }

    /// Compress every file of the shell's own frontend at its best, in the
    /// background, so the first person to load it waits for none of it.
    ///
    /// Brotli only: that is what every browser the frontend runs in asks
    /// for. Anything asked for before this reaches it is compressed quickly
    /// on the way, as any file is.
    pub fn precompress(self: &Arc<Self>, directory: &Path) {
        if self.budget == 0 {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let cache = self.clone();
        let directory = directory.to_path_buf();
        runtime.spawn(async move {
            let started = std::time::Instant::now();
            let files = tokio::task::spawn_blocking(move || files_under(&directory))
                .await
                .unwrap_or_default();
            for file in files {
                let Ok(body) = tokio::fs::read(&file).await else {
                    continue;
                };
                if (body.len() as u64) < SMALLEST
                    || body.len() > cache.budget
                    || image(file.to_string_lossy().as_ref())
                {
                    continue;
                }
                let body = Bytes::from(body);
                let hashed = body.clone();
                let Ok(key) =
                    tokio::task::spawn_blocking(move || key(&hashed, Encoding::Brotli)).await
                else {
                    continue;
                };
                {
                    let mut kept = cache.kept.lock().expect("never poisoned");
                    if !kept.waiting.insert(key) {
                        continue;
                    }
                }
                // Kept quickly first, so `at_best` has something to improve
                // and a request meanwhile finds it.
                if cache.get(&key).is_none() {
                    let raw = body.clone();
                    if let Ok(quick) =
                        tokio::task::spawn_blocking(move || Encoding::Brotli.quick(&raw)).await
                    {
                        cache.keep(key, Bytes::from(quick), false);
                    }
                }
                cache.at_best(key, body).await;
                cache
                    .kept
                    .lock()
                    .expect("never poisoned")
                    .waiting
                    .remove(&key);
            }
            let held = cache.held();
            tracing::info!(
                ms = started.elapsed().as_millis() as u64,
                kept_bytes = held.bytes,
                kept_files = held.entries,
                "compressed the frontend"
            );
        });
    }
}

fn files_under(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                found.push(entry.path());
            }
        }
    }
    found
}

/// An image by its name: not worth compressing again, and SVG, which is
/// text, is the exception.
fn image(name: &str) -> bool {
    let lowered = name.to_ascii_lowercase();
    [
        ".png", ".jpg", ".jpeg", ".gif", ".webp", ".avif", ".ico", ".bmp",
    ]
    .iter()
    .any(|extension| lowered.ends_with(extension))
}

/// Whether a response of this type is one to compress, whatever its size.
fn compressible(headers: &HeaderMap) -> bool {
    let kind = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    let never = (kind.starts_with("image/") && !kind.starts_with("image/svg+xml"))
        || kind.starts_with("text/event-stream")
        || kind.starts_with("application/grpc");
    !never
}

/// Add `Accept-Encoding` to `Vary`, unless it is there already.
fn vary(headers: &mut HeaderMap) {
    let varies = headers.get_all(header::VARY).iter().any(|value| {
        value.to_str().is_ok_and(|value| {
            value
                .split(',')
                .any(|name| name.trim().eq_ignore_ascii_case("accept-encoding"))
        })
    });
    if !varies {
        headers.append(header::VARY, HeaderValue::from_static("accept-encoding"));
    }
}

/// The layer: compress the answer as the request accepts, from what is kept
/// where it can.
pub async fn compress(
    State(cache): State<Arc<Compressed>>,
    request: Request,
    next: Next,
) -> Response {
    let accepted = Encoding::accepted(request.headers());
    // A `HEAD` has no body to compress, only a length that would be wrong.
    let get = request.method() == axum::http::Method::GET;
    let mut response = next.run(request).await;

    if cache.budget == 0
        || !get
        || response.status() != StatusCode::OK
        || response.headers().contains_key(header::CONTENT_ENCODING)
        || !compressible(response.headers())
    {
        return response;
    }
    // Whatever was chosen this time, a cache between here and the browser
    // must not hand it to a browser that asked differently.
    vary(response.headers_mut());

    let Some(encoding) = accepted else {
        return response;
    };
    // A file from disk says how long it is only in its header.
    let declared = response
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok()?.parse::<u64>().ok());
    let Some(size) = response.body().size_hint().exact().or(declared) else {
        return response;
    };
    if size < SMALLEST || size > cache.budget as u64 {
        return response;
    }

    let (mut parts, body) = response.into_parts();
    let body = match axum::body::to_bytes(body, size as usize).await {
        Ok(body) => body,
        Err(error) => {
            tracing::warn!(%error, "a file could not be read to compress it");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let compressed = cache.compressed(body, encoding).await;

    parts.headers.insert(
        header::CONTENT_ENCODING,
        HeaderValue::from_static(encoding.token()),
    );
    parts.headers.remove(header::CONTENT_LENGTH);
    // A range of the compressed bytes is not the range that was asked for.
    parts.headers.remove(header::ACCEPT_RANGES);
    Response::from_parts(parts, Body::from(compressed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accepts(value: &str) -> Option<Encoding> {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_str(value).unwrap(),
        );
        Encoding::accepted(&headers)
    }

    #[test]
    fn brotli_is_preferred_unless_weighed_lower() {
        assert_eq!(accepts("gzip, deflate, br, zstd"), Some(Encoding::Brotli));
        assert_eq!(accepts("gzip"), Some(Encoding::Gzip));
        assert_eq!(accepts("br;q=0.5, gzip"), Some(Encoding::Gzip));
        assert_eq!(accepts("br;q=0, gzip;q=0"), None);
        assert_eq!(accepts("identity, deflate"), None);
        assert_eq!(accepts("BR"), Some(Encoding::Brotli));
        assert_eq!(Encoding::accepted(&HeaderMap::new()), None);
    }

    #[test]
    fn the_budget_is_kept_by_forgetting_what_was_used_longest_ago() {
        let cache = Compressed::new(&CompressionConfig { cache_bytes: 3000 });
        let one = key(b"one", Encoding::Gzip);
        let two = key(b"two", Encoding::Gzip);
        let three = key(b"three", Encoding::Gzip);
        cache.keep(one, Bytes::from(vec![1; 1000]), false);
        cache.keep(two, Bytes::from(vec![2; 1000]), false);
        // Used, so `two` is now the one used longest ago.
        assert!(cache.get(&one).is_some());
        cache.keep(three, Bytes::from(vec![3; 1500]), false);

        assert!(cache.get(&one).is_some());
        assert!(cache.get(&two).is_none());
        assert!(cache.get(&three).is_some());
        assert_eq!(cache.held().bytes, 2500);

        // Bigger than the whole budget: not kept, and nothing lost for it.
        cache.keep(two, Bytes::from(vec![2; 3001]), false);
        assert!(cache.get(&two).is_none());
        assert_eq!(cache.held().entries, 2);
    }

    #[test]
    fn a_quick_compression_never_replaces_a_best_one() {
        let cache = Compressed::default();
        let one = key(b"one", Encoding::Brotli);
        cache.keep(one, Bytes::from_static(b"best"), true);
        cache.keep(one, Bytes::from_static(b"quick"), false);
        assert_eq!(cache.get(&one), Some((Bytes::from_static(b"best"), true)));
        assert_eq!(cache.held().bytes, 4);
    }

    #[test]
    fn the_same_bytes_are_one_entry_and_different_bytes_are_two() {
        assert_eq!(
            key(b"a file", Encoding::Brotli),
            key(b"a file", Encoding::Brotli)
        );
        assert_ne!(
            key(b"a file", Encoding::Brotli),
            key(b"a file", Encoding::Gzip)
        );
        assert_ne!(
            key(b"a file", Encoding::Brotli),
            key(b"a file!", Encoding::Brotli)
        );
    }

    #[test]
    fn best_is_smaller_than_quick() {
        let body: Vec<u8> = (0..200_000u32)
            .flat_map(|i| format!("item {} of {};", i % 97, i % 13).into_bytes())
            .collect();
        let quick = Encoding::Brotli.quick(&body);
        let best = Encoding::Brotli.best(&body).unwrap();
        assert!(
            best.len() < quick.len(),
            "{} >= {}",
            best.len(),
            quick.len()
        );
    }

    #[test]
    fn a_cache_of_a_few_bytes_is_a_typo() {
        assert!(CompressionConfig { cache_bytes: 0 }.check().is_ok());
        assert!(CompressionConfig { cache_bytes: 10 }.check().is_err());
        assert!(CompressionConfig::default().check().is_ok());
    }
}
