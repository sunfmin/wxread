//! XML as WeChat writes it, as wxread reads it: a document from its first
//! tag, with the declarations WeChat puts inside some and the bare `&` of
//! some links' addresses, and a node's children, text and attributes found
//! by name, each trimmed and None when empty. Every crate that reads
//! WeChat's XML reads it with these.

use roxmltree::{Document, Node};

/// What `read` makes of the XML document `content`, read as WeChat writes
/// it: from its first tag, without the XML declarations WeChat puts inside
/// some messages, and with an `&` that starts no entity, as in some links'
/// addresses. None when it is not XML even then.
pub fn xml<R>(content: &str, read: impl FnOnce(Node) -> Option<R>) -> Option<R> {
    let text = declarations_removed(content.trim());
    let escaped;
    let doc = match Document::parse(&text) {
        Ok(doc) => doc,
        Err(_) => {
            escaped = bare_ampersands_escaped(&text);
            Document::parse(&escaped).ok()?
        }
    };
    read(doc.root_element())
}

/// `text` without its `<?xml …?>` declarations.
fn declarations_removed(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once("<?xml") {
        out.push_str(before);
        rest = after.split_once("?>").map_or("", |(_, after)| after);
    }
    out.push_str(rest);
    out
}

/// `text` with each `&` that starts no entity (`&amp;`, `&#38;`) written
/// `&amp;`.
fn bare_ampersands_escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pieces = text.split('&');
    if let Some(first) = pieces.next() {
        out.push_str(first);
    }
    for piece in pieces {
        out.push('&');
        let entity = piece.split_once(';').is_some_and(|(name, _)| is_entity(name));
        if !entity {
            out.push_str("amp;");
        }
        out.push_str(piece);
    }
    out
}

/// Whether `&<name>;` is an entity XML reads: a name, or a character XML
/// allows by its number.
fn is_entity(name: &str) -> bool {
    let all =
        |s: &str, f: fn(&u8) -> bool| !s.is_empty() && s.len() <= 8 && s.bytes().all(|c| f(&c));
    // A character XML allows: not a control character but tab, newline
    // and return.
    let allowed = |n: Option<u32>| {
        n.and_then(char::from_u32)
            .is_some_and(|c| !c.is_control() || matches!(c, '\t' | '\n' | '\r'))
    };
    match (name.strip_prefix("#x"), name.strip_prefix('#')) {
        (Some(hex), _) => {
            all(hex, u8::is_ascii_hexdigit) && allowed(u32::from_str_radix(hex, 16).ok())
        }
        (None, Some(decimal)) => all(decimal, u8::is_ascii_digit) && allowed(decimal.parse().ok()),
        (None, None) => {
            name.starts_with(|c: char| c.is_ascii_alphabetic())
                && all(name, u8::is_ascii_alphanumeric)
        }
    }
}

/// `node`'s child named `path`'s last part, through each part before it:
/// `child(n, "appmsg/refermsg/svrid")`.
pub fn child<'a, 'i>(node: Node<'a, 'i>, path: &str) -> Option<Node<'a, 'i>> {
    let mut at = node;
    for name in path.split('/') {
        at = named(at, name).into_iter().next()?;
    }
    Some(at)
}

/// `node`'s children named `name`, in order.
pub fn named<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
    let mut found = Vec::new();
    for c in node.children() {
        if c.has_tag_name(name) {
            found.push(c);
        }
    }
    found
}

/// The text of `path` under `node`, trimmed: None when it is absent or
/// empty.
pub fn text(node: Node, path: &str) -> Option<String> {
    child(node, path).and_then(inner)
}

/// The text a node holds, trimmed, CDATA or not: None when empty.
pub fn inner(node: Node) -> Option<String> {
    let mut t = String::new();
    for n in node.children() {
        if let Some(text) = n.text().filter(|_| n.is_text()) {
            t.push_str(text);
        }
    }
    Some(t.trim().to_string()).filter(|t| !t.is_empty())
}

/// Attribute `name` of `node`, trimmed: None when it is absent or empty.
pub fn attr(node: Node, name: &str) -> Option<String> {
    node.attribute(name).map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_document_reads_past_its_declarations_and_bare_ampersands() {
        let doc = "<msg><?xml version=\"1.0\"?><a k=\" v \"><b>x & y &amp; z</b><b><![CDATA[ two ]]></b><c> </c></a></msg>";
        let read = xml(doc, |root| {
            let a = child(root, "a")?;
            Some((attr(a, "k"), text(root, "a/b"), named(a, "b").len(), text(a, "c"), text(a, "d")))
        });
        assert_eq!(read, Some((Some("v".into()), Some("x & y & z".into()), 2, None, None)));
        assert_eq!(xml("not xml", |_| Some(())), None);
    }
}
