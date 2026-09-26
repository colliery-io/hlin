//! Calling a widget the way the shell calls it, for its tests.
//!
//! A widget's rules are only worth testing through the same door the shell
//! uses: a real socket, a token minted by a real [`Issuer`] for each person,
//! an ordinary token on a read and on a write one bound to its method and
//! path, and a fresh `Idempotency-Key`. [`start`] runs a widget that way on a
//! loopback port; [`Running`] makes the calls.
//!
//! ```text
//! let counter = testing::start(widget(), Counter::default(), api()).await;
//! let alice = testing::person("u-alice", "Alice");
//! let answer = counter.write(&alice, "POST", "/api/counter/bump", Some(json!({"by": 1}))).await;
//! assert_eq!(answer.status, 200);
//! ```

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Router;
use futures::StreamExt;
use hlin_identity::{BoundRequest, Issuer, Principal, Verifier};
use serde_json::Value;

use crate::files::ModuleFiles;
use crate::platform::Platform;
use crate::site::{Site, site};
use crate::widget::Widget;

/// A widget running on a loopback port, and a shell's worth of calling it.
pub struct Running {
    /// Where Hlin's surface answers, as the shell's `base_url` would name it:
    /// `http://127.0.0.1:{port}/hlin`. Every path a test sends is under it.
    pub base: String,
    /// The widget's origin, `http://127.0.0.1:{port}`: its own UI and its own
    /// `/api/` ([`Running::own`]).
    pub origin: String,
    /// The platform id tokens are minted for.
    pub id: String,
    /// Where the fallback's data is, and which envelope it promises.
    fallback: Option<(String, String)>,
    issuer: Issuer,
    client: reqwest::Client,
}

/// What came back.
#[derive(Debug, Clone)]
pub struct Answered {
    /// The status code.
    pub status: u16,
    /// The body as JSON, `Null` where there was none, or a string where it
    /// was not JSON.
    pub body: Value,
    /// Whether the widget said this was an answer given before.
    pub replayed: bool,
}

/// Somebody, as the shell's sign-in would describe them.
pub fn person(sub: &str, name: &str) -> Principal {
    Principal::new(sub).with_name(name)
}

/// Run `widget` from `state`, with no module built, as the platform named
/// after its panel.
pub async fn start<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
) -> Running {
    start_with(widget, state, api, ModuleFiles::none()).await
}

/// [`start`], serving these module files.
pub async fn start_with<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
    module: ModuleFiles,
) -> Running {
    start_site(widget, state, api, module, Site::default()).await
}

/// [`start_with`], with the origin laid out as `layout` says: a UI of its own,
/// a local user for its own `/api/`, another base for Hlin's surface.
pub async fn start_site<S: Send + 'static>(
    widget: Widget<S>,
    state: S,
    api: Router<Platform<S>>,
    module: ModuleFiles,
    layout: Site,
) -> Running {
    let issuer = Issuer::generate("hlin");
    let id = widget.panel.to_string();
    let fallback = widget.fallback.as_ref().map(|fallback| {
        (
            format!("/{}", widget.fallback_data()),
            fallback.envelope.to_string(),
        )
    });
    let verifier = Arc::new(Verifier::with_keys("hlin", issuer.jwks()));
    let platform = Platform::new(id.clone(), widget, state, verifier, module);

    let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("a loopback port");
    let origin = format!("http://{}", listener.local_addr().expect("an address"));
    let base = format!("{origin}{}", layout.hlin_base);
    let app = site(platform, api, layout);
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("the widget serves");
    });

    Running {
        base,
        origin,
        id,
        fallback,
        issuer,
        client: reqwest::Client::new(),
    }
}

/// A key nobody has used, in this process.
pub fn fresh_key() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!("key-{}", NEXT.fetch_add(1, Ordering::Relaxed))
}

impl Running {
    /// The ordinary token the shell sends on a read.
    pub fn read_token(&self, who: &Principal) -> String {
        self.issuer.mint(who, &self.id).expect("a token")
    }

    /// A token bound to this method and path, as the shell sends on a write.
    pub fn write_token(&self, who: &Principal, method: &str, path: &str) -> String {
        let request = BoundRequest::new(method, path).expect("a request to bind to");
        self.issuer
            .mint_bound(who, &self.id, &request)
            .expect("a bound token")
    }

    /// A read, as `who`.
    pub async fn get(&self, who: &Principal, path: &str) -> Answered {
        let token = self.read_token(who);
        self.send("GET", path, Some(&token), None, None).await
    }

    /// A write, as `who`, bound and with a fresh key.
    pub async fn write(
        &self,
        who: &Principal,
        method: &str,
        path: &str,
        body: Option<Value>,
    ) -> Answered {
        self.write_keyed(who, method, path, body, &fresh_key())
            .await
    }

    /// A write, as `who`, bound, with this key: for a retry.
    pub async fn write_keyed(
        &self,
        who: &Principal,
        method: &str,
        path: &str,
        body: Option<Value>,
        key: &str,
    ) -> Answered {
        let token = self.write_token(who, method, path);
        self.send(method, path, Some(&token), Some(key), body).await
    }

    /// Anything at all under Hlin's base: for the requests the shell would
    /// never send.
    pub async fn send(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        key: Option<&str>,
        body: Option<Value>,
    ) -> Answered {
        self.send_to(format!("{}{}", self.base, path), method, token, key, body)
            .await
    }

    /// A request to the widget's origin rather than Hlin's base, with no
    /// token, as the widget's own UI sends it: `path` from the root.
    pub async fn own(
        &self,
        method: &str,
        path: &str,
        key: Option<&str>,
        body: Option<Value>,
    ) -> Answered {
        self.send_to(format!("{}{}", self.origin, path), method, None, key, body)
            .await
    }

    async fn send_to(
        &self,
        url: String,
        method: &str,
        token: Option<&str>,
        key: Option<&str>,
        body: Option<Value>,
    ) -> Answered {
        let mut request = self.client.request(method.parse().expect("a method"), url);
        if let Some(token) = token {
            request = request.header(hlin_identity::IDENTITY_HEADER, token);
        }
        if let Some(key) = key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            request = request.json(&body);
        }
        let response = request.send().await.expect("the widget answers");
        let status = response.status().as_u16();
        let replayed = response.headers().contains_key("idempotency-replayed");
        let text = response.text().await.expect("a body");
        let body = if text.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Answered {
            status,
            body,
            replayed,
        }
    }

    /// The fallback's data as `who` would see it, read the way the shell
    /// reads it: an envelope that must be the one the manifest promised.
    pub async fn fallback(&self, who: &Principal) -> hlin_manifest::Envelope {
        self.fallback_asking(who, "").await
    }

    /// [`Running::fallback`], with this query on the request, as the shell
    /// adds `from`, `to` and `step` for a panel that declares `time_range`.
    pub async fn fallback_asking(&self, who: &Principal, query: &str) -> hlin_manifest::Envelope {
        let (path, promised) = self.fallback.as_ref().expect("this widget has a fallback");
        let token = self.read_token(who);
        let query = if query.is_empty() {
            String::new()
        } else {
            format!("?{query}")
        };
        let response = self
            .client
            .get(format!("{}{}{}", self.base, path, query))
            .header(hlin_identity::IDENTITY_HEADER, token)
            .send()
            .await
            .expect("the widget answers");
        assert_eq!(response.status(), 200, "the fallback answers");
        let bytes = response.bytes().await.expect("a body");
        hlin_manifest::parse_envelope(&bytes, promised)
            .unwrap_or_else(|defect| panic!("the shell would refuse this fallback: {defect}"))
    }

    /// Open the event stream as `who`, and wait until the widget is holding
    /// it open, so a write made next cannot be announced to nobody.
    pub async fn listen(&self, who: &Principal) -> Events {
        let token = self.read_token(who);
        let response = self
            .client
            .get(format!("{}/api/events", self.base))
            .header(hlin_identity::IDENTITY_HEADER, token)
            .send()
            .await
            .expect("the widget answers");
        assert_eq!(response.status(), 200, "the event stream opens");
        Events {
            body: Box::pin(response.bytes_stream()),
            seen: String::new(),
        }
    }
}

/// An open event stream.
pub struct Events {
    body: std::pin::Pin<Box<dyn futures::Stream<Item = reqwest::Result<axum::body::Bytes>> + Send>>,
    seen: String,
}

impl Events {
    /// The data of the next `changed` event, waiting up to five seconds.
    pub async fn changed(&mut self) -> Option<Value> {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(at) = self.seen.find("event: changed\n") {
                let rest = &self.seen[at..];
                if let Some(end) = rest.find("\n\n") {
                    let event = rest[..end].to_string();
                    self.seen = rest[end + 2..].to_string();
                    let data = event.lines().find_map(|line| line.strip_prefix("data: "))?;
                    return serde_json::from_str(data).ok();
                }
            }
            let chunk = tokio::time::timeout_at(deadline, self.body.next())
                .await
                .ok()??
                .ok()?;
            self.seen.push_str(&String::from_utf8_lossy(&chunk));
        }
    }
}
