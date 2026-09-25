//! Who may do what to a post.
//!
//! Decided from two things only: the claims in the token the shell minted, and
//! this platform's own data and configuration. That is the whole of
//! attribute-based access control as a platform behind Hlin does it. The shell
//! authenticates; it holds no roles and is told nothing about these rules
//! ([[HLIN-I-0010]] decisions 3 and 5).
//!
//! Kept apart from the HTTP handlers so the rules can be read in one place,
//! which is what a platform team copying this wants to find first.
//!
//! - Anyone signed in reads. A verified token is enough, so there is no
//!   function for it.
//! - Posting needs an `email` claim at the configured domain, and not to be
//!   muted here.
//! - Editing needs to be the post's author, by `sub`, and not to be muted:
//!   rewriting a post is posting.
//! - Deleting needs to be the post's author. A muted author may still delete,
//!   because taking one's own words down is never something to refuse.

use std::collections::BTreeSet;
use std::fmt;

use hlin_identity::Claims;

use crate::posts::Post;

/// This platform's rules, as configured when it started.
#[derive(Debug, Clone)]
pub struct Rules {
    /// The one email domain whose people may post, lowercased.
    domain: String,
    /// Email addresses that may not post here, lowercased.
    muted: BTreeSet<String>,
}

impl Rules {
    /// Rules for a feed where people at `domain` may post, apart from `muted`.
    ///
    /// Both compared without regard to case, since neither a domain nor, in
    /// practice, any mailbox anybody uses is case-sensitive, and a rule that a
    /// capital letter defeats is not a rule.
    pub fn new<I, S>(domain: &str, muted: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Self {
            domain: domain.trim().to_lowercase(),
            muted: muted
                .into_iter()
                .map(|email| email.as_ref().trim().to_lowercase())
                .collect(),
        }
    }

    /// The domain whose people may post.
    pub fn domain(&self) -> &str {
        &self.domain
    }

    /// Whether this person may write a new post.
    pub fn may_post(&self, claims: &Claims) -> Result<(), Forbidden> {
        if !self.at_domain(claims) {
            return Err(Forbidden::OutsideDomain {
                domain: self.domain.clone(),
            });
        }
        if self.is_muted(claims) {
            return Err(Forbidden::Muted);
        }
        Ok(())
    }

    /// Whether this person may change what `post` says.
    pub fn may_edit(&self, claims: &Claims, post: &Post) -> Result<(), Forbidden> {
        if post.author.id != claims.sub {
            return Err(Forbidden::NotTheAuthor { doing: "edit" });
        }
        if self.is_muted(claims) {
            return Err(Forbidden::Muted);
        }
        Ok(())
    }

    /// Whether this person may take `post` down.
    pub fn may_delete(&self, claims: &Claims, post: &Post) -> Result<(), Forbidden> {
        if post.author.id != claims.sub {
            return Err(Forbidden::NotTheAuthor { doing: "delete" });
        }
        Ok(())
    }

    /// Whether the token's `email` is at this feed's domain.
    ///
    /// Compared on the whole domain after the last `@`, because every looser
    /// comparison has a well-known way round it: a suffix match lets
    /// `notexample.com` in, and a substring match lets `example.com.evil.org`.
    /// No email at all is not at the domain.
    fn at_domain(&self, claims: &Claims) -> bool {
        let Some(email) = claims.email.as_deref() else {
            return false;
        };
        match email.trim().rsplit_once('@') {
            Some((mailbox, domain)) => !mailbox.is_empty() && domain.to_lowercase() == self.domain,
            None => false,
        }
    }

    fn is_muted(&self, claims: &Claims) -> bool {
        claims
            .email
            .as_deref()
            .is_some_and(|email| self.muted.contains(&email.trim().to_lowercase()))
    }
}

/// Why this platform refused, in words a person can act on.
///
/// Rendered as a 403 whose body the shell passes to the module unchanged, so
/// the words are this platform's and the module shows them as they are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Forbidden {
    /// The person's email is not at the domain whose people may post.
    OutsideDomain {
        /// The domain that may.
        domain: String,
    },
    /// The person is on this platform's muted list.
    Muted,
    /// The post is somebody else's.
    NotTheAuthor {
        /// What they tried to do to it.
        doing: &'static str,
    },
}

impl fmt::Display for Forbidden {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutsideDomain { domain } => {
                write!(formatter, "Only people at {domain} can post here")
            }
            Self::Muted => formatter.write_str("You have been muted on this feed"),
            Self::NotTheAuthor { doing } => {
                write!(formatter, "Only the author can {doing} this post")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::posts::Author;

    fn claims(sub: &str, email: Option<&str>) -> Claims {
        Claims {
            iss: "hlin".to_string(),
            sub: sub.to_string(),
            aud: "feed".to_string(),
            iat: 0,
            exp: 0,
            jti: "test".to_string(),
            name: None,
            email: email.map(str::to_string),
            groups: vec![],
            extra: Default::default(),
        }
    }

    fn post_by(sub: &str) -> Post {
        Post {
            id: "p1".to_string(),
            author: Author {
                id: sub.to_string(),
                name: sub.to_string(),
            },
            body: "hello".to_string(),
            posted_at: chrono::Utc::now(),
            edited_at: None,
        }
    }

    #[test]
    fn only_the_whole_domain_after_the_last_at_counts() {
        let rules = Rules::new("example.com", Vec::<String>::new());
        let cases = [
            (Some("alice@example.com"), true),
            (Some("ALICE@Example.COM"), true),
            (Some("carol@elsewhere.org"), false),
            (Some("eve@notexample.com"), false),
            (Some("eve@example.com.evil.org"), false),
            (Some("eve@sub.example.com"), false),
            (Some("@example.com"), false),
            (Some("example.com"), false),
            (None, false),
        ];
        for (email, allowed) in cases {
            assert_eq!(
                rules.may_post(&claims("u", email)).is_ok(),
                allowed,
                "{email:?}"
            );
        }
    }

    #[test]
    fn a_muted_person_may_not_post_or_edit_but_may_delete_their_own() {
        let rules = Rules::new("example.com", ["Mo@Example.com"]);
        let mo = claims("mo", Some("mo@example.com"));
        let theirs = post_by("mo");

        assert_eq!(rules.may_post(&mo), Err(Forbidden::Muted));
        assert_eq!(rules.may_edit(&mo, &theirs), Err(Forbidden::Muted));
        assert_eq!(rules.may_delete(&mo, &theirs), Ok(()));
    }

    #[test]
    fn authorship_is_the_stable_id_not_the_name_or_the_email() {
        let rules = Rules::new("example.com", Vec::<String>::new());
        let mut impostor = claims("someone-else", Some("alice@example.com"));
        impostor.name = Some("alice".to_string());

        assert_eq!(
            rules.may_edit(&impostor, &post_by("alice")),
            Err(Forbidden::NotTheAuthor { doing: "edit" })
        );
    }
}
