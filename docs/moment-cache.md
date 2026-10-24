# The name WeChat caches a Moment's media under

WeChat for Mac 4.x caches the images and videos of the Moments (朋友圈) it
has shown under the account's directory (the one above `db_storage`):

    cache/<YYYY-MM>/Sns/Img/<k[..2]>/<k[2..]>          an image's copies
    cache/<YYYY-MM>/Sns/Video/<k[..2]>/<k[2..]>.mp4    a video (its thumbnail the .jpg beside it)

`k` is the MD5, in lowercase hex, of `<post id>_<media id>_<n>`:

| `n` | what it is                                         |
|-----|----------------------------------------------------|
| 2   | the image itself, at the size `<size>` says        |
| 1   | its thumbnail (the `<thumb>` one); for a video, its cover |
| 6   | a smaller copy, mostly 240 pixels on its longer side |
| 3   | a video, `.mp4`                                    |

The ids are the ones `sns/sns.db`'s `SnsTimeLine.content` XML says: the
post's is `TimelineObject/id` (the row's `tid` read as unsigned, the same
number), the media's is its `media/id`, both in decimal. The image files
have no extension and are sealed in V2 under the account's image key, as a
chat's images are (`wxread-media`'s `image`). `YYYY-MM` is the month the
post was posted, not the month WeChat cached it: files written on October
9th of a post of September lie under `2026-09`. Its month is taken in the
Mac's own time zone here; no post's month differs between it and UTC, so whether WeChat
takes it in UTC is not told: a reader looks in every month.

`wxread-media` holds the image's rule as `Media::MomentImage`.

## The evidence

On one account, WeChat for Mac 4.1.5 (build 4066647863), 119 posts in
`SnsTimeLine`, counted by `docs/moment-cache.py`:

```sh
uv run docs/moment-cache.py --keys <file of `sudo wxread keys`>
```

| count                                              | value |
|----------------------------------------------------|-------|
| image files under `cache/*/Sns/Img`                | 223   |
| … named by the rule                                | 209   |
| … by `_2` / `_1` / `_6`                            | 44 / 121 / 44 |
| images of the posts (`media/type` 2)               | 244   |
| … whose image (`_2`) is cached                     | 44    |
| … whose thumbnail (`_1`) is cached                 | 109   |
| video files under `cache/*/Sns/Video` (`.mp4`)     | 2     |
| … named by the rule (`_3`)                         | 1     |
| videos of the posts (`media/type` 6)               | 28    |
| … whose video is cached                            | 1     |
| named files in the month their post was posted     | 210 of 210 |

Each copy is the one its suffix says: decrypted, 42 of the 44 `_2` files
are exactly the width and height the media's `<size>` says, every `_6` is
240 pixels on its longer side but three (472), and of the 121 `_1` files
the longest side is 100 pixels at the least and 1252 at the most. `_1`,
`_2` and `_6` are cached together or `_1` alone, never another way.

The 14 image files and the 1 video the rule does not name are named by
nothing `sns.db` holds: no pair of the numbers in any post's XML, no URL,
token, key or text of it, with any suffix 0 to 15. Six of them are three
pairs of the same image under two names; their sizes (540×720 and the like,
one 60×60) are none of the rule's copies'. They are guessed, not shown, to
be of posts WeChat showed and did not keep in `SnsTimeLine` (a contact's
album page); either way the rule names no file wrongly.

## What else was tried

Over every media of every post, with no file named by any (and the post id
as `<id>`, as `tid` and as `tid` read unsigned):

- the MD5 of the media's `<url>`, of its `<thumb>`, of each with
  `https://`, and of each with `_<n>` for `n` 0 to 11;
- the MD5 of the `md5`, `key` and `token` attributes of `<url>` and
  `<thumb>`, and of the media id alone;
- `<post id>_<media id>` with no suffix, and `_0`, `_4`, `_5`, `_7` to `_11`.

`<url>`'s `md5` attribute is not the MD5 of the decrypted image either
(0 of 44), as SIWX found on Windows.
