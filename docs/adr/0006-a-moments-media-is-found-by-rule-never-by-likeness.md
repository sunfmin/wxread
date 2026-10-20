# A Moment's media is found by the rule WeChat names it by, never by likeness

`wxread moments` lists the Moments WeChat has loaded on this Mac, as `sns/sns.db` holds them: each its `<SnsDataItem>` XML, the Moment as WeChat's server sent it and its likes and comments as WeChat last loaded them. Nothing else is there: what was never shown on this Mac, and what WeChat has dropped, is not listed, and the help says so before anything else. The Moments are listed newest first by their id, whose first 41 bits are the millisecond each was posted in (as SIWX found), so a `next` is one Moment's id. A Moment whose XML does not read is listed with its id, author and content, never dropped, as a message is (0004).

Each medium is named `<the Moment's id>:<its place>`, which `wxread file get` takes beside a message's id: it cannot be mistaken for one, which is digits alone. WeChat caches what it loaded of a Moment under `cache/<YYYY-MM>/Sns`, each file named by an MD5. A medium is said to be held, and is written, only when that name is derived from the Moment by the rule WeChat names it by: a video's is the MD5 of `<the Moment's id>_<the medium's id>_3`, as wx-workbench found it. Other readers pair a Moment's images with cached files by their size and the time they were cached; wxread does not, because a guess that is wrong hands an agent another Moment's photo as this one's, and nothing in the answer could tell it so. Until the rule for images is found (#28), a Moment's image is missing, with what makes WeChat load it, like any file WeChat does not hold (0005).

## Consequences

- A Moment's image is answered missing until #28 finds the rule; `wxread moments --help` says so.
- A file written for a Moment's medium is its own, or nothing is written: there is no confidence to read.
- What the Moments' CDN addresses serve is not fetched: they expire, and the cached copy is what WeChat showed.
