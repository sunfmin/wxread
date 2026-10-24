//! What a sender is called, the way WeChat shows them: the account's
//! remark for them, else the name they gave themselves in the group, else
//! their own name.

use std::collections::BTreeMap;

use serde_json::Value;
pub use wxread_answer::person;
use wxread_contacts::display_name;

use crate::Chat;

/// What a contact is called: the account's remark for it, and its own name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contact {
    pub remark: Option<String>,
    pub nick_name: Option<String>,
}

/// The names a chat's senders are shown by.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Names {
    /// Every contact the account knows, by id.
    pub contacts: BTreeMap<String, Contact>,
    /// The names members gave themselves in the chat, when it is a group.
    pub in_group: BTreeMap<String, String>,
}

impl Names {
    /// What `id` is shown as: None when nothing names it.
    pub fn of(&self, id: &str) -> Option<String> {
        let contact = self.contacts.get(id);
        let shown = display_name(
            contact.and_then(|c| c.remark.as_deref()),
            self.in_group.get(id).map(String::as_str),
            contact.and_then(|c| c.nick_name.as_deref()),
        );
        (!shown.is_empty()).then_some(shown)
    }

    /// `id` as an answer names them ([`person`]): shown as [`Names::of`]
    /// says, else by `given`, the name the message itself gives them.
    pub fn person(&self, id: Option<&str>, given: Option<&str>) -> Value {
        let shown = id.and_then(|id| self.of(id));
        person(id, shown.as_deref().or(given.filter(|g| !g.is_empty())))
    }

    /// `found`, a person a message's kind names (`{id, display_name}`, its
    /// `display_name` the name the message gives them), named in place as
    /// [`Names::person`] names them.
    pub fn name(&self, found: &mut Value) {
        if found.is_null() {
            return;
        }
        let id = found["id"].as_str().map(str::to_string);
        let given = found["display_name"].as_str().map(str::to_string);
        *found = self.person(id.as_deref(), given.as_deref());
    }

    /// Who other than the account goes by `name` in `chat`: in a direct
    /// chat, the contact it is with; in a group, the one member whose
    /// remark, name in the group or own name it is. None when no member
    /// does, or more than one.
    pub fn who(&self, chat: &Chat, name: &str) -> Option<String> {
        if !chat.is_group() {
            return Some(chat.id.clone());
        }
        let goes_by = |id: &str| {
            let contact = self.contacts.get(id);
            [
                contact.and_then(|c| c.remark.as_deref()),
                self.in_group.get(id).map(String::as_str),
                contact.and_then(|c| c.nick_name.as_deref()),
            ]
            .contains(&Some(name))
        };
        let mut found = self.in_group.keys().filter(|id| goes_by(id));
        match (found.next(), found.next()) {
            (Some(id), None) => Some(id.clone()),
            _ => None,
        }
    }
}

/// The account's own id, out of its directory's name `dir` (WeChat names
/// it `<id>_<4 characters>`): the name without that suffix when `known`
/// knows it as an id, else the name whole.
pub fn account_id(dir: &str, known: impl Fn(&str) -> bool) -> String {
    match dir.rsplit_once('_') {
        Some((id, suffix))
            if suffix.len() == 4
                && suffix.bytes().all(|b| b.is_ascii_alphanumeric())
                && known(id) =>
        {
            id.to_string()
        }
        _ => dir.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(remark: &str, nick: &str) -> Contact {
        Contact { remark: Some(remark.into()), nick_name: Some(nick.into()) }
    }

    #[test]
    fn a_remark_comes_before_the_group_name_and_that_before_the_own_name() {
        let mut names = Names::default();
        names.contacts.insert("a".into(), contact("Mum", "Mary"));
        names.contacts.insert("b".into(), contact("", "Bob"));
        names.contacts.insert("c".into(), contact("", "Cat"));
        names.in_group.insert("a".into(), "Mary G".into());
        names.in_group.insert("b".into(), "Bobby".into());
        names.in_group.insert("d".into(), "Dan".into());
        let shown: Vec<_> = ["a", "b", "c", "d", "e"].iter().map(|id| names.of(id)).collect();
        let want = [Some("Mum"), Some("Bobby"), Some("Cat"), Some("Dan"), None];
        assert_eq!(shown, want.map(|s| s.map(String::from)));
    }

    #[test]
    fn the_accounts_id_is_its_directory_without_the_suffix_when_known() {
        let known = |id: &str| id == "wxid_me";
        assert_eq!(account_id("wxid_me_1a2b", known), "wxid_me");
        assert_eq!(account_id("wxid_other_1a2b", known), "wxid_other_1a2b");
        assert_eq!(account_id("wxid_me", known), "wxid_me");
    }
}
