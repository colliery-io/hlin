//! The posts, and the three things that change them.
//!
//! Held in memory, because this is a reference and a database would be most of
//! what a reader had to read. Everything that changes a post goes through
//! [`Posts`], which checks [`Rules`] first, so there is no path to a write that
//! skips the rules.

use chrono::{DateTime, Utc};
use hlin_identity::Claims;
use hlin_manifest::envelope::{Column, ColumnType, Envelope, Records};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::rules::{Forbidden, Rules};

/// The longest post this feed accepts, in characters.
pub const MAX_BODY_CHARS: usize = 500;

/// One post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Post {
    /// Stable, and never reused after a delete.
    pub id: String,
    /// Who wrote it.
    pub author: Author,
    /// What it says.
    pub body: String,
    /// When it was first posted.
    pub posted_at: DateTime<Utc>,
    /// When it was last edited, if it has been.
    pub edited_at: Option<DateTime<Utc>>,
}

/// Who wrote a post.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Author {
    /// The token's `sub`: the stable id authorship is decided by.
    pub id: String,
    /// What to call them, as the token said when they posted.
    pub name: String,
}

impl Author {
    /// The author a token describes.
    ///
    /// `name` for display where the identity provider gave one, then the
    /// email, then the bare id, so a post always says *somebody* wrote it.
    pub fn from_claims(claims: &Claims) -> Self {
        let name = claims
            .name
            .clone()
            .or_else(|| claims.email.clone())
            .unwrap_or_else(|| claims.sub.clone());
        Self {
            id: claims.sub.clone(),
            name,
        }
    }
}

/// What a write asks for: the post's new words.
#[derive(Debug, Clone, Deserialize)]
pub struct Draft {
    /// What the post should say.
    pub body: String,
}

impl Draft {
    /// The body, trimmed, or why it cannot be posted.
    fn checked(&self) -> Result<String, Problem> {
        let body = self.body.trim();
        if body.is_empty() {
            return Err(Problem::Invalid("A post needs some words"));
        }
        if body.chars().count() > MAX_BODY_CHARS {
            return Err(Problem::Invalid("A post can be at most 500 characters"));
        }
        Ok(body.to_string())
    }
}

/// Why a write did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// The request itself is wrong. The words are for a person.
    Invalid(&'static str),
    /// There is no such post.
    NotFound,
    /// The rules say no.
    Forbidden(Forbidden),
}

impl From<Forbidden> for Problem {
    fn from(forbidden: Forbidden) -> Self {
        Self::Forbidden(forbidden)
    }
}

/// Every post, oldest first, and the next id to hand out.
#[derive(Debug, Clone)]
pub struct Posts {
    posts: Vec<Post>,
    next: u64,
}

impl Posts {
    /// A feed holding these posts to begin with.
    ///
    /// Ids handed out later start after the highest numbered one here, so a
    /// seeded feed never hands out an id it already holds.
    pub fn new(mut posts: Vec<Post>) -> Self {
        posts.sort_by_key(|post| post.posted_at);
        let next = posts
            .iter()
            .filter_map(|post| post.id.strip_prefix('p')?.parse::<u64>().ok())
            .max()
            .unwrap_or(0)
            + 1;
        Self { posts, next }
    }

    /// Every post, newest first.
    pub fn newest_first(&self) -> Vec<Post> {
        self.posts.iter().rev().cloned().collect()
    }

    /// Write a new post, if this person may.
    pub fn create(
        &mut self,
        rules: &Rules,
        claims: &Claims,
        draft: &Draft,
    ) -> Result<Post, Problem> {
        rules.may_post(claims)?;
        let body = draft.checked()?;

        let post = Post {
            id: format!("p{}", self.next),
            author: Author::from_claims(claims),
            body,
            posted_at: Utc::now(),
            edited_at: None,
        };
        self.next += 1;
        self.posts.push(post.clone());
        Ok(post)
    }

    /// Change what a post says, if this person may.
    ///
    /// The rules are checked before the draft, so somebody who may not edit a
    /// post learns that rather than that their words were too long.
    pub fn edit(
        &mut self,
        rules: &Rules,
        claims: &Claims,
        id: &str,
        draft: &Draft,
    ) -> Result<Post, Problem> {
        let post = self
            .posts
            .iter_mut()
            .find(|post| post.id == id)
            .ok_or(Problem::NotFound)?;
        rules.may_edit(claims, post)?;
        let body = draft.checked()?;

        post.body = body;
        post.edited_at = Some(Utc::now());
        Ok(post.clone())
    }

    /// Take a post down, if this person may.
    pub fn delete(&mut self, rules: &Rules, claims: &Claims, id: &str) -> Result<(), Problem> {
        let index = self
            .posts
            .iter()
            .position(|post| post.id == id)
            .ok_or(Problem::NotFound)?;
        rules.may_delete(claims, &self.posts[index])?;
        self.posts.remove(index);
        Ok(())
    }

    /// The posts as the shell draws them when the module cannot be loaded.
    ///
    /// `records.v1`, newest first. Only what a table needs: the module has the
    /// richer JSON, and this is the fallback.
    pub fn as_records(&self) -> Envelope {
        let rows = self
            .newest_first()
            .into_iter()
            .map(|post| {
                let mut row = Map::new();
                row.insert("author".to_string(), Value::String(post.author.name));
                row.insert("post".to_string(), Value::String(post.body));
                row.insert(
                    "posted".to_string(),
                    Value::String(post.posted_at.to_rfc3339()),
                );
                if let Some(edited) = post.edited_at {
                    row.insert("edited".to_string(), Value::String(edited.to_rfc3339()));
                }
                row
            })
            .collect();

        Envelope::Records(Records {
            columns: vec![
                column("author", "Author", ColumnType::String),
                column("post", "Post", ColumnType::String),
                column("posted", "Posted", ColumnType::Timestamp),
                column("edited", "Edited", ColumnType::Timestamp),
            ],
            rows,
            as_of: Some(Utc::now()),
            extra: Default::default(),
        })
    }
}

fn column(key: &str, label: &str, value_type: ColumnType) -> Column {
    Column {
        key: key.to_string(),
        label: label.to_string(),
        value_type,
        unit: None,
        extra: Default::default(),
    }
}

/// A few posts, so the feed is not empty the first time anybody looks.
///
/// Written by people who do not sign in to the demo, so nobody who does can
/// edit them. That is on purpose: a post that nobody present wrote is the
/// simplest way to see the author rule refuse.
pub fn seed() -> Vec<Post> {
    let now = Utc::now();
    let by = |id: &str, name: &str| Author {
        id: id.to_string(),
        name: name.to_string(),
    };
    vec![
        Post {
            id: "p1".to_string(),
            author: by("seed-dana", "Dana"),
            body: "Welcome to the feed. Anyone signed in can read it; people at \
                   example.com can post."
                .to_string(),
            posted_at: now - chrono::Duration::hours(3),
            edited_at: None,
        },
        Post {
            id: "p2".to_string(),
            author: by("seed-eli", "Eli"),
            body: "Only a post's author can edit or delete it, and the feed \
                   says so in its own words when you try."
                .to_string(),
            posted_at: now - chrono::Duration::hours(2),
            edited_at: None,
        },
        Post {
            id: "p3".to_string(),
            author: by("seed-dana", "Dana"),
            body: "The checklist is next door, for anything that wants doing \
                   rather than saying."
                .to_string(),
            posted_at: now - chrono::Duration::hours(1),
            edited_at: None,
        },
    ]
}
