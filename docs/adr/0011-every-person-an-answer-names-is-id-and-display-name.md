# Every person an answer names is its id and display name

Each command named the people in its answer its own way: a message's sender was `sender` and `sender_name`, a quote's `sender` and `name`, a card's `contact` and `name`, who joined, left, renamed or nudged a bare id, a Moment's author `{id, name}`, an article's official account `{id, name}`, a favorite's source `from`, `from_name`, `chat` and `chat_name`, a chat in `wxread chats` `name`, while `wxread contacts` and `wxread group` answered `display_name`. Each command also worked its names out apart, and not the same way: a search's chat name left out what names an unnamed group, and what a chat history or a forward gave a sender was kept over the account's remark. An agent could not read one shape everywhere, and a remarked contact appeared under two names, as an agent found trying the released 0.1.1 across commands on a real account.

Every person, group, official account and chat an answer names is now `{id, display_name}`, the same shape in every command, and `wxread --help` says the rule once. `id` is the contact's id, null when WeChat names someone only by a name (a forward's sender, a quote without its sender's id). `display_name` is the contact's as `wxread contacts` answers it: the account's remark, else (in a group's messages) the name the member set in it, else their own name, else the name the message, post or favorite itself gives; a group with no name of its own by its members' names; `""` for none, never null. The object is null when nothing names anyone (WeChat's own notices have no sender).

## Consequences

- One function in the effect crate (`contacts::display_names`) says what each contact is shown as, read the way `wxread contacts` reads it; `moments`, `articles`, `favorites`, `search`, `file get` and `voice` use it instead of their own reading of the contact table. Within a chat's messages, `wxread_messages::Names` adds the name in the group, as the glossary's display name says.
- One function makes the shape (`wxread_messages::names::person`); a kind's fields carry each person with the name the message gives them, and naming them is one step (`Names::name`) applied wherever a kind holds a person.
- A field that is a name but not a person keeps its own key: a renamed group's new `name`, a place's `name`, a file's `name`.
- `wxread voice` now reads contact/contact.db, for its chat's display name, so its keys must open it.
- `account`, the account's directory as doctor names it, is not a contact and stays a string.
