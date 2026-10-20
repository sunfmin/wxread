# A note sent into a chat is its items, as a favorite's note is, and its files lie under its own directory

A note (笔记) sent into a chat is an app message of type 24 whose `<recorditem>` keeps a `<recordinfo>` as a favorite keeps a note: a `<datalist>` of items, each with its `datatype` and `dataid`. `wxread messages` answers it as the kind `note`, `{items}`, each item read by the same code `wxread favorites` reads a favorite's note's items with (0008), so the two are one shape: its body (an htm file, its first item), its paragraphs, places, images and files, each as its kind. That reading lives in `wxread-favorites` and finds an item's file through a `Files` the caller gives it; the XML helpers both crates read with are the pure crate `wxread-xml`, so `wxread-messages` depends on `wxread-favorites` and not the other way.

WeChat for Mac 4.1 keeps the files of a note's items under the note's own directory in the chat's, `msg/attach/<md5 of the chat>/<YYYY-MM>/Rec/<dir>`, `<dir>` named in the message's `packed_info_data` (field 9, its field 1), each by its place in the note's list: an image at `Img/<place>` (sealed as a message's image is), its thumbnail `Img/<place>_t`; a file at `F/<place>/<its datatitle>`; the note's own body at `Dat/<place>/<its datatitle, else its place>`, neither sealed. This is the rule that named every file of the 21 notes of one account (a body, images, a GPX file), found from their rows and the files beside them, not by likeness. An item that carries a file is named `note:<message>:<item>`, the message's server id and the item's `dataid`, as a favorite's is `fav:<favorite>:<item>`; `wxread file get` takes it.

## Consequences

- No note of that account held a downloaded video or voice message, so where WeChat keeps one is not known: such an item answers `file_id` and `file_present` null rather than a guess, and `wxread file get` of one is an error saying so. Finding it from a real note is #59 (https://github.com/sunfmin/wxread/issues/59).
- A note's own id is not a file's: `wxread file get <its id>` is an error saying to pass one of its items' file_id.
- A note the server never saw (server id 0) names no file, since its file id is its message's.
- A note keeps no sender or time for its items, so theirs are null; WeChat's `<des>` summary of it is kept in `content`.
- `wxread search` reads a note WeChat has not indexed by its paragraphs and its places' and files' names; what WeChat indexed it by is its summary.
