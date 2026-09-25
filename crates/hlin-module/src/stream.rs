//! A streamed response body, read as it arrives.

use std::collections::BTreeMap;

use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use hlin_bridge::EndError;

use crate::Module;
use crate::request::{Answer, Refused};

/// How many bytes a reader asks for up front, and keeps asked for: the
/// specification's example of a `pull`. Enough for a log tail to flow without
/// a round trip per line, small enough that a module which stops reading holds
/// little.
pub const CREDIT_WINDOW: u64 = 256 * 1024;

/// What a streamed `fetch` produced.
#[derive(Debug)]
pub enum StreamReply {
    /// The platform is answering, and the body follows through the reader.
    Streaming {
        /// The platform's status.
        status: u16,
        /// The headers the shell passes back.
        headers: BTreeMap<String, String>,
        /// The body, as it arrives.
        body: BodyReader,
    },
    /// The platform answered whole after all.
    Answered(Answer),
    /// The shell refused the request itself.
    Refused(Refused),
}

pub(crate) enum StreamEvent {
    Chunk(Vec<u8>),
    End(Option<EndError>),
}

/// Why a stream stopped before its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamError {
    /// The shell ended it, for this reason.
    Ended(EndError),
    /// The module went away.
    Gone,
}

impl std::fmt::Display for StreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ended(reason) => write!(f, "the stream ended early: {reason:?}"),
            Self::Gone => f.write_str("the bridge went away"),
        }
    }
}

impl std::error::Error for StreamError {}

/// A streamed body.
///
/// Flow control is credit: the page sends only as many bytes as the module
/// has asked for. The reader asks for [`CREDIT_WINDOW`] when it is made, and
/// for as many again as it hands over each time it hands a chunk over, so what
/// is in flight never passes the window and a module that stops reading stops
/// the page, the shell and the platform in turn. Dropping the reader before
/// the end cancels the request.
#[derive(Debug)]
pub struct BodyReader {
    module: Module,
    re: String,
    events: UnboundedReceiver<StreamEvent>,
    finished: bool,
}

impl std::fmt::Debug for StreamEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Chunk(bytes) => write!(f, "Chunk({} bytes)", bytes.len()),
            Self::End(error) => write!(f, "End({error:?})"),
        }
    }
}

impl BodyReader {
    pub(crate) fn open(module: Module, re: String, events: UnboundedReceiver<StreamEvent>) -> Self {
        module.pull(&re, CREDIT_WINDOW);
        Self {
            module,
            re,
            events,
            finished: false,
        }
    }

    /// The next part of the body; `Ok(None)` at its end.
    pub async fn next(&mut self) -> Result<Option<Vec<u8>>, StreamError> {
        if self.finished {
            return Ok(None);
        }
        match self.events.next().await {
            Some(StreamEvent::Chunk(bytes)) => {
                if !bytes.is_empty() {
                    self.module.pull(&self.re, bytes.len() as u64);
                }
                Ok(Some(bytes))
            }
            Some(StreamEvent::End(None)) => {
                self.finished = true;
                Ok(None)
            }
            Some(StreamEvent::End(Some(reason))) => {
                self.finished = true;
                Err(StreamError::Ended(reason))
            }
            None => {
                self.finished = true;
                Err(StreamError::Gone)
            }
        }
    }

    /// Ends the stream now. The same as dropping the reader, said out loud.
    pub fn cancel(self) {}

    /// The id of the `fetch` this body answers, for a log.
    pub fn id(&self) -> &str {
        &self.re
    }
}

impl Drop for BodyReader {
    fn drop(&mut self) {
        if !self.finished {
            self.module.cancel_stream(&self.re);
        }
    }
}
