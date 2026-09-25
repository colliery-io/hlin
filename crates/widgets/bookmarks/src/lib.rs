//! Widget 15 of twenty ([[HLIN-I-0012]]): shared links.
//!
//! The team's bookmarks, newest first. Shared: a link one person adds is on
//! everybody's panel as soon as it is announced.
//!
//! | Method | Path | What |
//! |---|---|---|
//! | `GET` | `/api/bookmarks` | Every link, newest first, and which you may remove |
//! | `POST` | `/api/bookmarks` | `{ "url": "https://...", "title": "..." }`, the title optional |
//! | `DELETE` | `/api/bookmarks/{id}` | Remove one |
//!
//! Plus what every widget serves (`hlin_widget_support::router`), including
//! the fallback: the links as a `table`.
//!
//! The rules:
//!
//! - Anyone signed in may add a link: `http` or `https`, with a host, and no
//!   spaces. Without a title, the host is the title; a title is at most
//!   [`LONGEST_TITLE`] characters.
//! - A link already on the list is refused, however it is capitalised or
//!   whether it ends in a slash.
//! - At most [`MOST`] links: a list nobody can read is not shared knowledge.
//! - A link can be removed by whoever added it. The ones the list started
//!   with belong to the team, and anybody may remove them.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get};
use hlin_widget_support::envelope::{Column, ColumnType, Envelope, Records};
use hlin_widget_support::{
    Claims, Fallback, Platform, Refusal, Reply, Viewer, Widget, Write, display_name,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// The panel key, which is also the platform's default id.
pub const PANEL: &str = "bookmarks";

/// The most links the list holds.
pub const MOST: usize = 20;

/// The longest title, in characters.
pub const LONGEST_TITLE: usize = 80;

/// The longest link, in characters.
pub const LONGEST_URL: usize = 500;

/// Who the links the list starts with belong to.
const TEAM: &str = "The team";

/// The list.
#[derive(Debug, Clone, Default)]
pub struct Bookmarks {
    /// Oldest first; shown the other way round.
    links: Vec<Link>,
    next: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    id: u64,
    url: String,
    title: String,
    /// Who added it, by `sub`; `None` for the team's.
    sub: Option<String>,
    name: String,
}

/// The list as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shelf {
    /// Every link, newest first.
    pub links: Vec<Shown>,
    /// How many more fit.
    pub room: usize,
}

/// One link, as one person sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Shown {
    /// Its id, for removing it.
    pub id: u64,
    /// Where it goes.
    pub url: String,
    /// What it is called.
    pub title: String,
    /// The host, shown beside the title.
    pub host: String,
    /// Who added it, as they were called.
    pub added_by: String,
    /// Whether the viewer may remove it.
    pub removable: bool,
}

/// What adding a link asks for.
#[derive(Debug, Deserialize)]
pub struct Add {
    /// Where it goes.
    pub url: String,
    /// What to call it; the host, if not given.
    #[serde(default)]
    pub title: Option<String>,
}

/// The host of an `http` or `https` link, if it is one.
pub fn host(url: &str) -> Option<&str> {
    let (scheme, rest) = url.split_once("://")?;
    if !(scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then_some(host)
}

/// A link as the list compares it: without a trailing slash, in lower case.
fn same(url: &str) -> String {
    url.trim_end_matches('/').to_lowercase()
}

impl Bookmarks {
    /// The list the demo starts with.
    pub fn seed() -> Self {
        let mut bookmarks = Self::default();
        for (url, title) in [
            ("https://github.com/colliery-io/hlin", "Hlin"),
            (
                "https://doc.rust-lang.org/std/",
                "The Rust standard library",
            ),
            ("https://book.leptos.dev/", "The Leptos book"),
        ] {
            bookmarks.next += 1;
            bookmarks.links.push(Link {
                id: bookmarks.next,
                url: url.to_string(),
                title: title.to_string(),
                sub: None,
                name: TEAM.to_string(),
            });
        }
        bookmarks
    }

    /// The list as `claims` sees it.
    pub fn shelf(&self, claims: &Claims) -> Shelf {
        Shelf {
            links: self
                .links
                .iter()
                .rev()
                .map(|link| Shown {
                    id: link.id,
                    url: link.url.clone(),
                    title: link.title.clone(),
                    host: host(&link.url).unwrap_or_default().to_string(),
                    added_by: link.name.clone(),
                    removable: link.sub.as_ref().is_none_or(|sub| *sub == claims.sub),
                })
                .collect(),
            room: MOST.saturating_sub(self.links.len()),
        }
    }

    /// Add a link as `claims`, if the rules allow. Answers its id.
    pub fn add(&mut self, claims: &Claims, url: &str, title: Option<&str>) -> Result<u64, Refusal> {
        let url = url.trim();
        let Some(host) = host(url) else {
            return Err(Refusal::bad_request(
                "A bookmark is a web link, starting http:// or https://",
            ));
        };
        if url.chars().any(char::is_whitespace) || url.chars().count() > LONGEST_URL {
            return Err(Refusal::bad_request("That does not look like a link"));
        }
        let title = match title.map(str::trim) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => host.to_string(),
        };
        if title.chars().count() > LONGEST_TITLE {
            return Err(Refusal::bad_request(format!(
                "A title is at most {LONGEST_TITLE} characters"
            )));
        }
        if self.links.iter().any(|link| same(&link.url) == same(url)) {
            return Err(Refusal::conflict("That link is already here"));
        }
        if self.links.len() >= MOST {
            return Err(Refusal::conflict(format!(
                "{MOST} links is the most; remove one first"
            )));
        }
        self.next += 1;
        self.links.push(Link {
            id: self.next,
            url: url.to_string(),
            title,
            sub: Some(claims.sub.clone()),
            name: display_name(claims),
        });
        Ok(self.next)
    }

    /// Remove link `id`, if `claims` may.
    pub fn remove(&mut self, claims: &Claims, id: u64) -> Result<(), Refusal> {
        let Some(at) = self.links.iter().position(|link| link.id == id) else {
            return Err(Refusal::not_found("There is no such link"));
        };
        if self.links[at]
            .sub
            .as_ref()
            .is_some_and(|sub| *sub != claims.sub)
        {
            return Err(Refusal::forbidden(format!(
                "Only {} can remove this link",
                self.links[at].name
            )));
        }
        self.links.remove(at);
        Ok(())
    }
}

/// What the shell is told about this widget.
pub fn widget() -> Widget<Bookmarks> {
    Widget {
        panel: PANEL,
        name: "Bookmarks",
        icon: "bookmark",
        title: "Bookmarks",
        description: "The team's links, shared",
        shared: true,
        fallback: Some(Fallback {
            kind: "table",
            envelope: "records.v1",
            refresh_ms: None,
            data: |bookmarks, claims| Envelope::Records(as_table(&bookmarks.shelf(claims))),
        }),
        built: concat!(env!("CARGO_MANIFEST_DIR"), "/module/dist"),
    }
}

/// The links as a table, for the shell to draw where the module is not.
fn as_table(shelf: &Shelf) -> Records {
    let column = |key: &str, label: &str| Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type: ColumnType::String,
        unit: None,
        extra: Default::default(),
    };
    Records {
        columns: vec![
            column("title", "Title"),
            column("url", "Link"),
            column("added_by", "Added by"),
        ],
        rows: shelf
            .links
            .iter()
            .map(|link| {
                let row =
                    json!({ "title": link.title, "url": link.url, "added_by": link.added_by });
                row.as_object().expect("an object").clone()
            })
            .collect(),
        as_of: None,
        extra: Default::default(),
    }
}

/// This widget's own routes.
pub fn api() -> Router<Platform<Bookmarks>> {
    Router::new()
        .route("/api/bookmarks", get(read).post(add))
        .route("/api/bookmarks/{id}", delete(remove))
}

async fn read(State(platform): State<Platform<Bookmarks>>, Viewer(claims): Viewer) -> Response {
    axum::Json(platform.read(|bookmarks| bookmarks.shelf(&claims))).into_response()
}

async fn add(State(platform): State<Platform<Bookmarks>>, write: Write) -> Response {
    platform.write(&write, |bookmarks, claims| {
        let asked: Add =
            write.json("A bookmark is { \"url\": \"https://...\", \"title\": \"...\" }")?;
        bookmarks.add(claims, &asked.url, asked.title.as_deref())?;
        Ok(Reply::created(bookmarks.shelf(claims)))
    })
}

async fn remove(
    State(platform): State<Platform<Bookmarks>>,
    Path(id): Path<u64>,
    write: Write,
) -> Response {
    platform.write(&write, |bookmarks, claims| {
        bookmarks.remove(claims, id)?;
        Ok(Reply::ok(bookmarks.shelf(claims)))
    })
}
