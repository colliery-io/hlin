//! Reading a platform's answer without trusting how long it is.
//!
//! [[HLIN-S-0002]] makes any envelope over 1 MiB malformed, and the check for
//! that was correct — but it ran on a buffer that had already been allocated to
//! whatever length the platform chose to send. `reqwest` has no body limit
//! unless one is asked for, so the shell would happily allocate a gigabyte
//! before deciding a megabyte was too much, once per panel per refresh.
//!
//! That is the one place a misbehaving platform could take the shell down with
//! it, which is exactly backwards: the shell exists to degrade gracefully when
//! a platform has a bad day.
//!
//! Two guards, because either alone has a hole. `Content-Length` is refused
//! before a byte is read, which costs nothing and handles the honest case. And
//! the body is then read in chunks against a running total, because
//! `Content-Length` is optional, can be absent under chunked transfer encoding,
//! and is in any case a claim by the same party whose length is in question.

use bytes::Bytes;
use futures::StreamExt;

/// Why a body could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TooMuch {
    /// It was longer than the caller allows.
    ///
    /// `bytes` is what was seen before giving up, which is the declared length
    /// when the header was believed and the running total otherwise — so it is
    /// a floor, never an overstatement.
    Oversized {
        /// How much had been seen when this was decided.
        bytes: usize,
        /// The limit that was exceeded.
        limit: usize,
    },
    /// The connection failed partway through.
    Interrupted,
}

impl std::fmt::Display for TooMuch {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Oversized { bytes, limit } => {
                write!(
                    formatter,
                    "body is at least {bytes} bytes, over the {limit} byte limit"
                )
            }
            Self::Interrupted => {
                write!(formatter, "the connection failed partway through the body")
            }
        }
    }
}

/// Read a response body, giving up once it passes `limit`.
///
/// Abandoning the response drops the connection, which is the point: a platform
/// streaming without end costs the shell one refused request rather than its
/// memory.
pub async fn read_bounded(response: reqwest::Response, limit: usize) -> Result<Bytes, TooMuch> {
    // The cheap check first. A platform that says how long its answer is and is
    // telling the truth never reaches the loop below.
    if let Some(declared) = response.content_length()
        && declared > limit as u64
    {
        return Err(TooMuch::Oversized {
            bytes: declared.min(usize::MAX as u64) as usize,
            limit,
        });
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| TooMuch::Interrupted)?;

        // Checked before extending, so the allocation never exceeds the limit
        // by more than one chunk.
        if body.len() + chunk.len() > limit {
            return Err(TooMuch::Oversized {
                bytes: body.len() + chunk.len(),
                limit,
            });
        }

        body.extend_from_slice(&chunk);
    }

    Ok(Bytes::from(body))
}
