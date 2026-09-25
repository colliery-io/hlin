//! A module's streamed response, as the shell hands it to its own page
//! (specification HLIN-S-0007, *Streaming*).
//!
//! The shell enforces a stream's limits — how long it may go quiet, how fast
//! it may deliver — because the shell is the one holding the platform's
//! connection. But it is the page that tells the module why a stream ended,
//! in `end`, and an HTTP body has no way to say why it stopped: a connection
//! cut for idleness and one the platform dropped look the same from a
//! `fetch`. So the body of a streamed answer from `/p/` is framed. Each piece
//! of the platform's body travels as a data frame, as it arrives, and the
//! shell closes every stream it ends with an end frame naming the reason. A
//! body that stops without one was cut somewhere between, which the page
//! reports as `unreachable`.
//!
//! A frame is one kind byte, a four-byte big-endian length, and that many
//! bytes. Deliberately trivial: the page decodes it in WebAssembly, a chunk at
//! a time, and holding a partial frame is the only buffering it needs.

/// The request header the page sets to ask `/p/` for a streamed answer, and
/// the response header the shell sets on an answer it is streaming.
///
/// Answered with it, the body is frames; without it — a refusal of the
/// shell's, made before the platform was asked — the body is whole.
pub const STREAM_HEADER: &str = "x-hlin-stream";

/// A frame carrying part of the platform's body.
const DATA: u8 = 0;

/// The last frame of a streamed body, with why it ended.
const END: u8 = 1;

/// How long a frame header is.
const HEADER: usize = 5;

/// Why the shell ended a stream, as the end frame says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The platform's body ended.
    Finished,
    /// Nothing arrived for `stream_idle_seconds`.
    Idle,
    /// More arrived than `stream_bytes_per_second` allows.
    Rate,
    /// The platform's connection failed partway.
    Unreachable,
    /// A reason from a newer shell. The page reads it as a failure it cannot
    /// name.
    Unrecognised,
}

impl Ended {
    fn as_str(self) -> &'static str {
        match self {
            Self::Finished => "",
            Self::Idle => "idle",
            Self::Rate => "rate",
            Self::Unreachable => "unreachable",
            Self::Unrecognised => "unrecognised",
        }
    }

    fn parse(reason: &[u8]) -> Self {
        match reason {
            b"" => Self::Finished,
            b"idle" => Self::Idle,
            b"rate" => Self::Rate,
            b"unreachable" => Self::Unreachable,
            _ => Self::Unrecognised,
        }
    }
}

/// One frame of a streamed body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Frame {
    /// Part of the platform's body, in order.
    Data(Vec<u8>),
    /// The end, and why.
    End(Ended),
}

fn header(kind: u8, len: usize) -> [u8; HEADER] {
    let len = u32::try_from(len).expect("a frame is under four gigabytes");
    let [a, b, c, d] = len.to_be_bytes();
    [kind, a, b, c, d]
}

/// Part of the platform's body, framed.
pub fn data(bytes: &[u8]) -> Vec<u8> {
    let mut framed = Vec::with_capacity(HEADER + bytes.len());
    framed.extend_from_slice(&header(DATA, bytes.len()));
    framed.extend_from_slice(bytes);
    framed
}

/// The end of a streamed body, framed.
pub fn end(why: Ended) -> Vec<u8> {
    let reason = why.as_str().as_bytes();
    let mut framed = Vec::with_capacity(HEADER + reason.len());
    framed.extend_from_slice(&header(END, reason.len()));
    framed.extend_from_slice(reason);
    framed
}

/// Reads frames out of a body that arrives in pieces of any size.
///
/// Holds at most one partial frame, so what it buffers is bounded by the
/// largest piece the shell sent, which is bounded by what the platform sent
/// at once.
#[derive(Debug, Default)]
pub struct Decoder {
    held: Vec<u8>,
    ended: bool,
}

impl Decoder {
    /// A decoder at the start of a body.
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the next piece of the body, and return every frame it completed.
    ///
    /// A frame of a kind this page does not know is skipped, so a newer shell
    /// can add one. Anything after the end frame is ignored.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Frame> {
        let mut frames = Vec::new();
        if self.ended {
            return frames;
        }
        self.held.extend_from_slice(bytes);
        let mut at = 0;
        while self.held.len() - at >= HEADER {
            let kind = self.held[at];
            let len = u32::from_be_bytes([
                self.held[at + 1],
                self.held[at + 2],
                self.held[at + 3],
                self.held[at + 4],
            ]) as usize;
            if self.held.len() - at - HEADER < len {
                break;
            }
            let body = &self.held[at + HEADER..at + HEADER + len];
            at += HEADER + len;
            match kind {
                DATA if !body.is_empty() => frames.push(Frame::Data(body.to_vec())),
                END => {
                    frames.push(Frame::End(Ended::parse(body)));
                    self.ended = true;
                    self.held.clear();
                    return frames;
                }
                _ => {}
            }
        }
        self.held.drain(..at);
        frames
    }

    /// Whether the end frame has been read.
    pub fn ended(&self) -> bool {
        self.ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_decode_to_what_was_framed_however_the_body_is_cut() {
        let mut body = data(b"data: one\n\n");
        body.extend(data(b"data: two\n\n"));
        body.extend(end(Ended::Idle));

        for size in [1, 2, 5, 7, 64] {
            let mut decoder = Decoder::new();
            let frames: Vec<Frame> = body
                .chunks(size)
                .flat_map(|piece| decoder.push(piece))
                .collect();
            assert_eq!(
                frames,
                vec![
                    Frame::Data(b"data: one\n\n".to_vec()),
                    Frame::Data(b"data: two\n\n".to_vec()),
                    Frame::End(Ended::Idle),
                ],
                "cut every {size} bytes"
            );
            assert!(decoder.ended());
        }
    }

    #[test]
    fn every_reason_survives_the_trip() {
        for why in [
            Ended::Finished,
            Ended::Idle,
            Ended::Rate,
            Ended::Unreachable,
        ] {
            assert_eq!(Decoder::new().push(&end(why)), vec![Frame::End(why)]);
        }
    }

    #[test]
    fn a_reason_from_a_newer_shell_is_still_an_end() {
        let mut framed = header(END, 5).to_vec();
        framed.extend_from_slice(b"storm");
        assert_eq!(
            Decoder::new().push(&framed),
            vec![Frame::End(Ended::Unrecognised)]
        );
    }

    #[test]
    fn a_frame_of_an_unknown_kind_is_skipped() {
        let mut body = header(9, 3).to_vec();
        body.extend_from_slice(b"new");
        body.extend(data(b"old"));
        assert_eq!(
            Decoder::new().push(&body),
            vec![Frame::Data(b"old".to_vec())]
        );
    }

    #[test]
    fn nothing_after_the_end_is_read() {
        let mut decoder = Decoder::new();
        let mut body = end(Ended::Finished);
        body.extend(data(b"late"));
        assert_eq!(decoder.push(&body), vec![Frame::End(Ended::Finished)]);
        assert!(decoder.push(&data(b"later")).is_empty());
    }

    #[test]
    fn a_partial_frame_is_all_that_is_held() {
        let mut decoder = Decoder::new();
        let framed = data(&[7; 100]);
        assert!(decoder.push(&framed[..50]).is_empty());
        assert_eq!(decoder.held.len(), 50);
        assert_eq!(decoder.push(&framed[50..]), vec![Frame::Data(vec![7; 100])]);
        assert!(decoder.held.is_empty());
    }
}
