//! The deploy log's components: the log, as it happens, newest at the top.
//!
//! Mounted twice, unchanged: at the root of the deploy log's own origin by
//! `hlin-widget-deploys-ui`, and in a Hlin panel by
//! `hlin-widget-deploys-module`. The first widget that streams (HLIN-S-0007,
//! *Streaming*). It asks for the log with `widget.stream`, and reads it chunk
//! by chunk as it arrives: on its own page a streamed `fetch` of its own
//! `/api/deploys/log`; in a panel the SDK's `BodyReader`, which asks the shell
//! for more only as this hands chunks over. Either way the request is
//! cancelled if the body is dropped before the end. Chunks are not lines:
//! they fall however the network delivered them, so [`Splitter`] puts lines
//! back together.
//!
//! The log never ends by itself, so an end is always a reason: the shell's
//! `idle` or `rate`, the platform unreachable, or the platform refusing. Each
//! is said in words, and the components follow again a few seconds later,
//! from the last line they have (`?after=`), so a reconnect neither repeats
//! nor skips. An end because the widget itself is going stops following.
//!
//! Nothing here uses `widget.load`: the log is not something to fetch again
//! when told of a change, it is the change.

use std::time::Duration;

use hlin_widget_ui::{Chunks, Ended, Request, Streamed, Widget, platform_words, use_widget};
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;

/// The deploy log's own look, against the `--hlin-*` tokens and the shared
/// classes in `hlin-widget-ui`'s stylesheet.
pub const STYLE: &str = include_str!("../style.css");

/// How many lines the components keep. The panel shows about a dozen.
const KEPT: usize = 40;

/// How long to wait before following the log again after it stopped.
const AGAIN: Duration = Duration::from_secs(3);

/// One line of the log, as the widget writes it.
#[derive(Debug, Clone, PartialEq, Deserialize)]
struct Line {
    seq: u64,
    at: String,
    service: String,
    version: String,
    level: String,
    text: String,
}

/// Whole lines out of a body that arrives in chunks, however they fall.
#[derive(Debug, Default)]
struct Splitter {
    partial: Vec<u8>,
}

impl Splitter {
    /// The lines this chunk completes. A line that does not parse is skipped:
    /// one bad line should not stop the log.
    fn push(&mut self, chunk: &[u8]) -> Vec<Line> {
        self.partial.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(end) = self.partial.iter().position(|byte| *byte == b'\n') {
            let text: Vec<u8> = self.partial.drain(..=end).collect();
            if let Ok(line) = serde_json::from_slice(&text[..end]) {
                lines.push(line);
            }
        }
        lines
    }
}

/// Where following the log has got to.
#[derive(Debug, Clone, PartialEq)]
enum Following {
    /// Asking for it.
    Opening,
    /// Lines are arriving.
    Live,
    /// It stopped, for this reason; following again shortly.
    Stopped(String),
}

/// Why the log ended early, in words for a person.
fn ended(reason: Ended) -> String {
    match reason {
        Ended::Idle => "The log went quiet for too long.",
        Ended::Rate => "The log was sending more than Hlin allows.",
        Ended::Unreachable => "The deploy log could not be reached.",
        Ended::Gone => "The log stopped.",
    }
    .to_string()
}

/// The newest `KEPT` of `lines` plus `more`, in order, without repeats.
fn keep(lines: &mut Vec<Line>, more: Vec<Line>) {
    let last = lines.last().map(|line| line.seq);
    lines.extend(
        more.into_iter()
            .filter(|line| last.is_none_or(|last| line.seq > last)),
    );
    let excess = lines.len().saturating_sub(KEPT);
    lines.drain(..excess);
}

/// Follow the log until it stops, then again after [`AGAIN`], for as long as
/// the widget lives.
fn follow(widget: Widget, lines: RwSignal<Vec<Line>>, state: RwSignal<Following>) {
    spawn_local(async move {
        let mut request = Request::get("/api/deploys/log");
        if let Some(after) = lines.with_untracked(|lines| lines.last().map(|line| line.seq)) {
            request = request.query(format!("after={after}"));
        }
        let why = match widget.stream(request).await {
            Streamed::Streaming(body) => {
                state.set(Following::Live);
                match read(body, lines).await {
                    Some(why) => why,
                    // The widget is going: nobody to follow it for.
                    None => return,
                }
            }
            // A refusal the platform decided, gathered whole and said in its
            // own words.
            Streamed::Answered(answer) => platform_words(&answer),
            Streamed::Trouble(trouble) => trouble.words,
        };
        state.set(Following::Stopped(why));
        set_timeout(move || follow(widget, lines, state), AGAIN);
    });
}

/// Read the log into `lines` until it ends, and say why it did; `None` if it
/// ended because the widget is going.
async fn read(mut body: Box<dyn Chunks>, lines: RwSignal<Vec<Line>>) -> Option<String> {
    let mut splitter = Splitter::default();
    loop {
        match body.next().await {
            Ok(Some(chunk)) => {
                let more = splitter.push(&chunk);
                if !more.is_empty() {
                    lines.update(|lines| keep(lines, more));
                }
            }
            Ok(None) => return Some("The log ended.".to_string()),
            Err(Ended::Gone) => return None,
            Err(reason) => return Some(ended(reason)),
        }
    }
}

/// `HH:MM:SS` in the browser's own time zone.
fn local_time(at: &str) -> String {
    let date = js_sys::Date::new(&js_sys::JsString::from(at).into());
    if date.get_time().is_nan() {
        return String::new();
    }
    format!(
        "{:02}:{:02}:{:02}",
        date.get_hours(),
        date.get_minutes(),
        date.get_seconds()
    )
}

/// The whole widget.
#[component]
pub fn Deploys() -> impl IntoView {
    let widget = use_widget();
    let lines = RwSignal::new(Vec::<Line>::new());
    let state = RwSignal::new(Following::Opening);
    follow(widget, lines, state);

    let status = move || match state.get() {
        Following::Opening => "Opening the log…".to_string(),
        Following::Live => "Live".to_string(),
        Following::Stopped(why) => format!("{why} Trying again…"),
    };
    let live = move || state.get() == Following::Live;

    view! {
        <style>{STYLE}</style>
        <p class="w-quiet deploys__status" class:deploys__status--live=live data-lines=move || lines.with(Vec::len)>
            {status}
        </p>
        <ol class="w-list deploys">
            <For
                each=move || lines.get().into_iter().rev()
                key=|line| line.seq
                children=|line| {
                    view! {
                        <li class=format!("deploys__line deploys__line--{}", line.level) data-seq=line.seq>
                            <time class="deploys__at">{local_time(&line.at)}</time>
                            <span class="deploys__what">
                                <b>{line.service}</b>
                                " "
                                <span class="deploys__version">{line.version}</span>
                                " "
                                {line.text}
                            </span>
                        </li>
                    }
                }
            />
        </ol>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(seq: u64) -> String {
        format!(
            r#"{{"seq":{seq},"at":"2026-09-25T12:00:00Z","service":"api","version":"1.0.{seq}","level":"info","text":"built"}}"#
        )
    }

    #[test]
    fn lines_are_put_back_together_however_the_chunks_fall() {
        let body = format!("{}\n{}\n", line(1), line(2));
        let (a, b) = body.as_bytes().split_at(body.len() / 2 + 3);
        let mut splitter = Splitter::default();
        let mut got = splitter.push(&a[..7]);
        got.extend(splitter.push(&a[7..]));
        got.extend(splitter.push(b));
        assert_eq!(got.iter().map(|line| line.seq).collect::<Vec<_>>(), [1, 2]);
        assert!(splitter.push(b"").is_empty());
    }

    #[test]
    fn a_line_that_does_not_parse_does_not_stop_the_log() {
        let body = format!("not json\n{}\n", line(3));
        let got = Splitter::default().push(body.as_bytes());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].seq, 3);
    }

    #[test]
    fn only_the_newest_are_kept_and_never_twice() {
        let parse = |seq| serde_json::from_str::<Line>(&line(seq)).unwrap();
        let mut lines = Vec::new();
        keep(&mut lines, (0..30).map(parse).collect());
        // A reconnect that overlaps what is already here.
        keep(&mut lines, (25..50).map(parse).collect());
        assert_eq!(lines.len(), KEPT);
        assert_eq!(lines.first().unwrap().seq, 50 - KEPT as u64);
        assert!(lines.windows(2).all(|pair| pair[1].seq == pair[0].seq + 1));
    }
}
