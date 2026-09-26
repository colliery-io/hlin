//! The client a widget's components use inside a Hlin panel: the SDK's
//! bridge to the shell.

use hlin_module::hlin_bridge::{EndError, Method as BridgeMethod, Refusal};
use hlin_module::{BodyReader, BridgeError, Module, StreamError, StreamReply};
use hlin_widget_ui::{
    Answer, Attempt, Chunks, Client, Ended, Method, Pending, Reply, Request, Streamed, TimeRange,
    Trouble,
};
use leptos::prelude::*;

use crate::KIT;
use crate::words::shell_words;

/// Connect to the shell, then mount `app`, a widget's components, with the
/// Hlin client for `panel`, and say `ready`.
///
/// `panel` is the panel key the widget's manifest declares; `changed` is
/// announced and listened for under it.
pub fn mount<F, V>(panel: &'static str, app: F)
where
    F: FnOnce() -> V + 'static,
    V: IntoView + 'static,
{
    console_error_panic_hook::set_once();
    // Not Leptos's `spawn_local`: its executor starts when something is
    // mounted, and nothing is until `init` has arrived.
    wasm_bindgen_futures::spawn_local(async move {
        // Resolves when the shell's `init` arrives.
        let module = hlin_module::connect().await;
        let ready = module.clone();
        hlin_widget_ui::mount(Hlin::new(module, panel), app);
        ready.ready(Some(KIT));
    });
}

/// The Hlin client: every request through the shell, which adds who is
/// asking; changes from the shell's relay of the platform's event stream.
pub struct Hlin {
    module: Module,
    panel: &'static str,
}

impl Hlin {
    /// A client for `panel` over this connection.
    pub fn new(module: Module, panel: &'static str) -> Self {
        Self { module, panel }
    }

    /// The SDK's handle, for anything the client does not cover.
    pub fn module(&self) -> &Module {
        &self.module
    }
}

impl Client for Hlin {
    fn fetch(&self, request: Request) -> Pending<Reply> {
        let module = self.module.clone();
        Box::pin(async move { reply(module.fetch(to_sdk(request)).await) })
    }

    fn attempt(&self, request: Request) -> Box<dyn Attempt> {
        Box::new(HlinAttempt(self.module.attempt(to_sdk(request))))
    }

    fn stream(&self, request: Request) -> Pending<Streamed> {
        let module = self.module.clone();
        Box::pin(async move {
            match module.fetch_stream(to_sdk(request)).await {
                Ok(StreamReply::Streaming {
                    status: 200..=299,
                    body,
                    ..
                }) => Streamed::Streaming(Box::new(HlinBody(body))),
                // A refusal the platform decided, streamed like any answer:
                // gathered whole, so it can be said in its own words.
                Ok(StreamReply::Streaming {
                    status, mut body, ..
                }) => {
                    let mut bytes = Vec::new();
                    while let Ok(Some(chunk)) = body.next().await {
                        bytes.extend(chunk);
                    }
                    Streamed::Answered(Answer {
                        status,
                        body: bytes,
                    })
                }
                Ok(StreamReply::Answered(answer)) => Streamed::Answered(Answer {
                    status: answer.status,
                    body: answer.body,
                }),
                Ok(StreamReply::Refused(refused)) => Streamed::Trouble(Trouble {
                    words: shell_words(&refused),
                    retry: retryable(refused.refusal),
                }),
                Err(error) => Streamed::Trouble(gone(error)),
            }
        })
    }

    fn changes(&self) -> Signal<u64> {
        let context = self.module.context();
        let changes = self.module.changes();
        let panel = self.panel;
        // A `changed` for this panel, by its sequence number, so two in a row
        // are still two reasons to look again.
        let here = Memo::new(move |_| {
            changes.with(|change| {
                change
                    .as_ref()
                    .filter(|change| change.panel == panel)
                    .map(|change| change.seq)
            })
        });
        Signal::derive(move || {
            context.with(|context| context.generation) + here.get().unwrap_or_default()
        })
    }

    /// The shell never infers a change from a write; the module says so, and
    /// the shell tells the platform's other modules.
    fn announce(&self) {
        self.module.changed(self.panel, Default::default());
    }

    fn read_only(&self) -> Signal<bool> {
        self.module.read_only()
    }

    fn visible(&self) -> Signal<bool> {
        self.module.visible()
    }

    fn time_range(&self) -> Signal<Option<TimeRange>> {
        let context = self.module.context();
        Signal::derive(move || {
            context.with(|context| {
                context.time_range.map(|range| TimeRange {
                    from_millis: range.from_millis,
                    to_millis: range.to_millis,
                })
            })
        })
    }

    fn restored(&self) -> Option<Vec<u8>> {
        self.module.restored()
    }

    fn on_suspend(&self, keep: Box<dyn Fn() -> Option<Vec<u8>>>) {
        self.module.on_suspend(keep);
    }
}

/// A write through the shell: the SDK's attempt, which holds the key.
struct HlinAttempt(hlin_module::Attempt);

impl Attempt for HlinAttempt {
    fn send(&self) -> Pending<Reply> {
        let attempt = self.0.clone();
        Box::pin(async move { reply(attempt.retry().await) })
    }
}

/// A streamed body through the shell.
struct HlinBody(BodyReader);

impl Chunks for HlinBody {
    fn next(&mut self) -> hlin_widget_ui::client::Borrowed<'_, Result<Option<Vec<u8>>, Ended>> {
        Box::pin(async move {
            self.0.next().await.map_err(|error| match error {
                StreamError::Ended(EndError::Idle) => Ended::Idle,
                StreamError::Ended(EndError::Rate) => Ended::Rate,
                StreamError::Ended(EndError::Unreachable) => Ended::Unreachable,
                _ => Ended::Gone,
            })
        })
    }
}

fn to_sdk(request: Request) -> hlin_module::Request {
    let method = match request.method {
        Method::Get => BridgeMethod::Get,
        Method::Post => BridgeMethod::Post,
        Method::Put => BridgeMethod::Put,
        Method::Patch => BridgeMethod::Patch,
        Method::Delete => BridgeMethod::Delete,
    };
    let mut sdk = hlin_module::Request::new(method, request.path).query(request.query);
    if let Some(body) = request.body {
        let content_type = request
            .content_type
            .unwrap_or_else(|| "application/octet-stream".to_string());
        sdk = sdk.body(&content_type, body);
    }
    sdk
}

fn reply(reply: Result<hlin_module::Reply, BridgeError>) -> Reply {
    match reply {
        Ok(hlin_module::Reply::Answered(answer)) => Ok(Answer {
            status: answer.status,
            body: answer.body,
        }),
        Ok(hlin_module::Reply::Refused(refused)) => Err(Trouble {
            words: shell_words(&refused),
            retry: retryable(refused.refusal),
        }),
        Err(error) => Err(gone(error)),
    }
}

/// Whether the shell's refusal was the network's, not a rule's.
fn retryable(refusal: Refusal) -> bool {
    matches!(
        refusal,
        Refusal::Unreachable | Refusal::Timeout | Refusal::TooMany
    )
}

fn gone(_: BridgeError) -> Trouble {
    Trouble {
        words: "The panel closed before the widget answered.".to_string(),
        retry: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_crosses_the_bridge_as_it_was_written() {
        let request = Request::post("/api/counter/bump")
            .query("x=1")
            .json(&serde_json::json!({ "by": 1 }))
            .unwrap();
        let sdk = to_sdk(request);
        assert_eq!(sdk.method(), BridgeMethod::Post);
        assert_eq!(sdk.path(), "/api/counter/bump");
    }

    #[test]
    fn the_shells_refusal_is_said_in_words_and_only_the_networks_is_retried() {
        let refused = |refusal| {
            Ok(hlin_module::Reply::Refused(hlin_module::Refused {
                refusal,
                status: 502,
                reason: "internal detail".to_string(),
            }))
        };
        let unreachable = reply(refused(Refusal::Unreachable)).unwrap_err();
        assert!(unreachable.retry);
        let read_only = reply(refused(Refusal::ReadOnly)).unwrap_err();
        assert!(!read_only.retry);
        assert!(!read_only.words.contains("internal detail"));
    }
}
