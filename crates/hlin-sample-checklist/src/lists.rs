//! The checklist's own data, and its rules about who may do what to it.
//!
//! This is the half of the platform that has nothing to do with Hlin. Hlin
//! tells the platform who is asking; everything about what that person may do
//! is decided here, from the lists this platform keeps and nothing else
//! ([[HLIN-I-0010]] decision 3). The shell holds no roles and is told nothing
//! about these rules (decision 5), so a platform team copying this crate can
//! read its whole authorization story in one file.
//!
//! The rules:
//!
//! - A list's **members** read it, add to it and cross items off.
//! - An item's **author**, or the list's **owner**, edits or deletes it.
//! - Anyone else is refused, with a sentence saying why.
//!
//! Membership is by email address, because that is what a person setting up a
//! list knows about their colleagues. Authorship is by the token's `sub`,
//! because an email can change hands and a stable id cannot.

use std::collections::BTreeMap;

use serde::Serialize;

/// The longest an item's text may be, in characters.
///
/// A checklist item is a line, not a document. The limit is here so a
/// mistaken paste does not become a row nobody can read.
pub const MAX_TEXT: usize = 200;

/// Who is asking, as far as this platform's rules care.
///
/// Built from a verified token and nothing else. The three fields are the
/// three things the rules and the display need, and deliberately not the
/// token's groups: this platform keeps its own membership rather than
/// borrowing an identity provider's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    /// The token's `sub`: stable, and what authorship is recorded against.
    pub id: String,
    /// What to show as an item's author.
    pub name: String,
    /// The token's `email`, lowercased, which membership is decided by.
    pub email: Option<String>,
}

impl Caller {
    /// The caller a verified token describes.
    pub fn from_claims(claims: &hlin_identity::Claims) -> Self {
        let email = claims.email.as_deref().map(str::to_lowercase);
        let name = claims
            .name
            .clone()
            .or_else(|| email.clone())
            .unwrap_or_else(|| claims.sub.clone());
        Self {
            id: claims.sub.clone(),
            name,
            email,
        }
    }
}

/// One line on a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// Unique across the whole platform, so an id means the same item
    /// whichever list a stale page thinks it is on.
    pub id: String,
    /// What needs doing.
    pub text: String,
    /// Whether it has been crossed off.
    pub done: bool,
    /// Who added it, for display.
    pub author: String,
    /// Who added it, for the rules: the token's `sub`.
    pub author_id: String,
}

/// A list, with its owner and members.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct List {
    /// How the list is addressed in paths and in the panel's `list` selection.
    pub id: String,
    /// What a person sees.
    pub name: String,
    /// The owner's email. The owner is always a member.
    pub owner: String,
    /// Every member's email, owner included.
    pub members: Vec<String>,
    /// The items, oldest first.
    #[serde(skip)]
    pub items: Vec<Item>,
}

impl List {
    fn has_member(&self, caller: &Caller) -> bool {
        caller
            .email
            .as_deref()
            .is_some_and(|email| self.members.iter().any(|member| member == email))
    }

    /// Whether this caller owns the list.
    pub fn is_owned_by(&self, caller: &Caller) -> bool {
        caller.email.as_deref() == Some(self.owner.as_str())
    }
}

/// Why this platform said no.
///
/// Each carries its status and a sentence in the platform's own words. The
/// sentence is written for the person who clicked, because the module shows it
/// to them as it is ([[HLIN-S-0007]], *Refusals*): nothing between here and
/// their screen rewords it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refused {
    /// No list has this id.
    NoSuchList(String),
    /// The list exists and this caller is not on it.
    NotAMember(String),
    /// The caller's token carried no email, so no list can be theirs.
    NoEmail,
    /// No item on this list has this id.
    NoSuchItem,
    /// A member tried to edit or delete an item they neither wrote nor own.
    NotYours,
    /// The request itself was not something this platform can act on.
    Invalid(&'static str),
}

impl Refused {
    /// The HTTP status this refusal is answered with.
    ///
    /// 403 for every rule and 404 for everything that is not there. None of
    /// these is a 401: the credential was fine, and a 401 would tell the shell
    /// its own configuration is broken ([[HLIN-S-0004]]).
    pub fn status(&self) -> u16 {
        match self {
            Self::NoSuchList(_) | Self::NoSuchItem => 404,
            Self::NotAMember(_) | Self::NoEmail | Self::NotYours => 403,
            Self::Invalid(_) => 400,
        }
    }

    /// What the person is told.
    pub fn message(&self) -> String {
        match self {
            Self::NoSuchList(id) => format!("There is no list called “{id}”."),
            Self::NotAMember(name) => {
                format!("Only members of {name} can see or change it, and you are not one.")
            }
            Self::NoEmail => "The checklist knows its members by email address, \
                 and your sign-in did not include one."
                .to_string(),
            Self::NoSuchItem => "That item is no longer on this list.".to_string(),
            Self::NotYours => "Only the person who added this item, or the list's owner, \
                 can edit or delete it."
                .to_string(),
            Self::Invalid(why) => (*why).to_string(),
        }
    }
}

/// Every list this platform keeps.
#[derive(Debug, Clone)]
pub struct Lists {
    lists: BTreeMap<String, List>,
    next_item: u64,
}

impl Lists {
    /// No lists at all.
    pub fn empty() -> Self {
        Self {
            lists: BTreeMap::new(),
            next_item: 1,
        }
    }

    /// The lists the demo starts with.
    ///
    /// `team` is shared by Alice and Bob, and `carol` is Carol's alone, so the
    /// demo's story has a list two people change together and one each of them
    /// is refused. The seeded items are written by each list's owner: they
    /// were added before anybody signed in, so there is no `sub` to record,
    /// and the owner rule is what lets them be changed.
    pub fn seeded() -> Self {
        let mut lists = Self::empty();
        let alice = "alice@example.com";
        let bob = "bob@example.com";
        let carol = "carol@elsewhere.org";

        lists.insert_list("team", "Team", alice, &[alice, bob]);
        for (text, done) in [
            ("Book a room for Thursday's review", false),
            ("Write up the retro notes", true),
            ("Order more coffee", false),
        ] {
            lists.seed_item("team", text, done, "Alice", alice);
        }

        lists.insert_list("carol", "Carol's list", carol, &[carol]);
        for (text, done) in [
            ("Renew the domain", false),
            ("Reply to the auditors", false),
        ] {
            lists.seed_item("carol", text, done, "Carol", carol);
        }

        lists
    }

    /// Add a list. Emails are lowercased, and the owner is made a member
    /// whether or not they were named, so "the owner is always a member" holds
    /// by construction rather than by care.
    pub fn insert_list(&mut self, id: &str, name: &str, owner: &str, members: &[&str]) {
        let owner = owner.to_lowercase();
        let mut everyone: Vec<String> = members.iter().map(|m| m.to_lowercase()).collect();
        if !everyone.contains(&owner) {
            everyone.insert(0, owner.clone());
        }
        self.lists.insert(
            id.to_string(),
            List {
                id: id.to_string(),
                name: name.to_string(),
                owner,
                members: everyone,
                items: Vec::new(),
            },
        );
    }

    fn seed_item(&mut self, list: &str, text: &str, done: bool, author: &str, author_id: &str) {
        let id = self.next_id();
        if let Some(list) = self.lists.get_mut(list) {
            list.items.push(Item {
                id,
                text: text.to_string(),
                done,
                author: author.to_string(),
                author_id: author_id.to_string(),
            });
        }
    }

    fn next_id(&mut self) -> String {
        let id = format!("i{}", self.next_item);
        self.next_item += 1;
        id
    }

    /// The lists this caller belongs to, in id order.
    ///
    /// Never an error: somebody on no lists has an empty picker, which is a
    /// true answer rather than a refusal.
    pub fn belonging_to(&self, caller: &Caller) -> Vec<&List> {
        self.lists
            .values()
            .filter(|list| list.has_member(caller))
            .collect()
    }

    /// One list, if this caller may read it.
    pub fn read(&self, caller: &Caller, list: &str) -> Result<&List, Refused> {
        let found = self
            .lists
            .get(list)
            .ok_or_else(|| Refused::NoSuchList(list.to_string()))?;
        if caller.email.is_none() {
            return Err(Refused::NoEmail);
        }
        if !found.has_member(caller) {
            return Err(Refused::NotAMember(found.name.clone()));
        }
        Ok(found)
    }

    /// The same check as [`Lists::read`], for a change.
    fn member_of(&mut self, caller: &Caller, list: &str) -> Result<&mut List, Refused> {
        self.read(caller, list)?;
        Ok(self.lists.get_mut(list).expect("read just found this list"))
    }

    /// Add an item, as its author. Members only.
    pub fn add(&mut self, caller: &Caller, list: &str, text: &str) -> Result<Item, Refused> {
        // Membership before the text, so somebody outside the list learns
        // that and nothing about what the list would have accepted.
        self.member_of(caller, list)?;
        let text = checked_text(text)?;
        let item = Item {
            id: self.next_id(),
            text,
            done: false,
            author: caller.name.clone(),
            author_id: caller.id.clone(),
        };
        self.lists
            .get_mut(list)
            .expect("member_of just found this list")
            .items
            .push(item.clone());
        Ok(item)
    }

    /// Cross an item off, or back on. Any member.
    ///
    /// Deliberately looser than editing: crossing off is how a shared list is
    /// used, and a list where only the author could tick their own items would
    /// be one nobody could help with.
    pub fn toggle(&mut self, caller: &Caller, list: &str, item: &str) -> Result<Item, Refused> {
        let found = self.member_of(caller, list)?;
        let item = found
            .items
            .iter_mut()
            .find(|candidate| candidate.id == item)
            .ok_or(Refused::NoSuchItem)?;
        item.done = !item.done;
        Ok(item.clone())
    }

    /// Change an item's text. Its author, or the list's owner.
    pub fn edit(
        &mut self,
        caller: &Caller,
        list: &str,
        item: &str,
        text: &str,
    ) -> Result<Item, Refused> {
        let found = self.member_of(caller, list)?;
        let index = mine_to_change(found, caller, item)?;
        let text = checked_text(text)?;
        let item = &mut found.items[index];
        item.text = text;
        Ok(item.clone())
    }

    /// Remove an item. Its author, or the list's owner.
    pub fn delete(&mut self, caller: &Caller, list: &str, item: &str) -> Result<Item, Refused> {
        let found = self.member_of(caller, list)?;
        let index = mine_to_change(found, caller, item)?;
        Ok(found.items.remove(index))
    }
}

/// Where an item is, if this caller may change it.
///
/// The existence check comes first, so a member asking about an item somebody
/// else just deleted is told it is gone rather than that it is not theirs.
fn mine_to_change(list: &List, caller: &Caller, item: &str) -> Result<usize, Refused> {
    let index = list
        .items
        .iter()
        .position(|candidate| candidate.id == item)
        .ok_or(Refused::NoSuchItem)?;
    if list.items[index].author_id == caller.id || list.is_owned_by(caller) {
        Ok(index)
    } else {
        Err(Refused::NotYours)
    }
}

fn checked_text(text: &str) -> Result<String, Refused> {
    let text = text.trim();
    if text.is_empty() {
        return Err(Refused::Invalid("An item needs some text."));
    }
    if text.chars().count() > MAX_TEXT {
        return Err(Refused::Invalid(
            "An item can be at most 200 characters long.",
        ));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caller(id: &str, email: Option<&str>) -> Caller {
        Caller {
            id: id.to_string(),
            name: id.to_string(),
            email: email.map(str::to_string),
        }
    }

    #[test]
    fn the_owner_is_a_member_even_when_not_named() {
        let mut lists = Lists::empty();
        lists.insert_list("x", "X", "Owner@Example.com", &["someone@example.com"]);
        let owner = caller("o", Some("owner@example.com"));
        assert!(lists.read(&owner, "x").is_ok());
    }

    #[test]
    fn membership_ignores_the_case_of_an_email() {
        let lists = Lists::seeded();
        let claims = hlin_identity::Claims {
            iss: "hlin".into(),
            sub: "b".into(),
            aud: "checklist".into(),
            iat: 0,
            exp: 0,
            jti: "j".into(),
            name: None,
            email: Some("Bob@Example.com".into()),
            groups: vec![],
            extra: Default::default(),
        };
        let bob = Caller::from_claims(&claims);
        assert!(lists.read(&bob, "team").is_ok());
        assert_eq!(bob.name, "bob@example.com", "falls back to the email");
    }

    #[test]
    fn a_caller_without_an_email_is_told_why_rather_than_that_they_are_not_a_member() {
        let lists = Lists::seeded();
        let nobody = caller("n", None);
        assert_eq!(lists.read(&nobody, "team").unwrap_err(), Refused::NoEmail);
    }

    #[test]
    fn text_is_trimmed_and_bounded() {
        let mut lists = Lists::seeded();
        let alice = caller("a", Some("alice@example.com"));
        let item = lists.add(&alice, "team", "  tidy  ").expect("adds");
        assert_eq!(item.text, "tidy");
        assert!(matches!(
            lists.add(&alice, "team", "   "),
            Err(Refused::Invalid(_))
        ));
        assert!(matches!(
            lists.add(&alice, "team", &"x".repeat(MAX_TEXT + 1)),
            Err(Refused::Invalid(_))
        ));
    }

    #[test]
    fn an_item_somebody_deleted_is_gone_not_forbidden() {
        let mut lists = Lists::seeded();
        let alice = caller("a", Some("alice@example.com"));
        let bob = caller("b", Some("bob@example.com"));
        let item = lists.add(&alice, "team", "mine").expect("adds");
        lists.delete(&alice, "team", &item.id).expect("deletes");
        assert_eq!(
            lists.edit(&bob, "team", &item.id, "yours now").unwrap_err(),
            Refused::NoSuchItem
        );
    }

    #[test]
    fn every_refusal_has_a_sentence_and_none_is_a_401() {
        for refusal in [
            Refused::NoSuchList("x".into()),
            Refused::NotAMember("Team".into()),
            Refused::NoEmail,
            Refused::NoSuchItem,
            Refused::NotYours,
            Refused::Invalid("An item needs some text."),
        ] {
            assert!(refusal.message().ends_with('.'), "{refusal:?}");
            assert_ne!(refusal.status(), 401, "{refusal:?}");
        }
    }
}
