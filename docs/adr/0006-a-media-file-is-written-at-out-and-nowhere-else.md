# A media file is written at --out, as the file it is, and nowhere else

`wxread file get <id> --out <path>` and `wxread avatar <id> --out <path>` are the only commands that write, and they write one file: the one `--out` names. An image is decrypted in memory (its `.dat` in whichever of WeChat's formats it is: the older XOR one, V1, or V2 under the account's image key), a sticker is fetched from WeChat's CDN into memory with macOS's own curl and checked against its MD5, a voice message's SILK is read out of its media database in memory; no temp file is made and nothing is written beside WeChat's files, since a decrypted image outlives the run that made it as surely as a decrypted database would (0003). A file `--out` already holds with the same bytes is left as it is, so a second run changes nothing. The answer is the file's path, size, SHA-256 and content type, read from its first bytes; WeChat's own wxgf is named `image/wxgf`, not converted.

The image key is derived without reading WeChat's memory, as wxkey found and bizkernel's wechat-intake reads it: the first 16 hex digits of the MD5 of `<code><account id>`, `code` from the names of WeChat's kvcomm files beside the data directory, the account id with and without WeChat's four-character suffix. Which pair it is is told by the account's first V2 image, the one it must open. The formats and the derivation are the pure crate `wxread-media`, checked by Flux and tested on their own (0002).

A file WeChat does not hold (it holds only a thumbnail, never downloaded the file, or cleared it) is an answer, exit 0: `{"missing": true, "thumbnail", "fix"}`, the fix what makes WeChat fetch it (open the message in WeChat). When WeChat holds the thumbnail, it is written at `--out` in the file's place and answered under `thumbnail`: the most WeChat holds is the most an agent can see, and `missing` says it is not the file. An agent working through a chat would otherwise stop at the first image WeChat never downloaded.

## Consequences

- `wxread messages` says of each media message whether its file and thumbnail are there (`file_present`, `thumbnail_present`), read from where WeChat keeps them; a sticker's is whether it names its CDN address, since telling more would mean fetching it.
- A sticker needs the network; one WeChat's CDN no longer serves is missing, since the copy WeChat caches is sealed under a key nobody has read.
- An image sealed under a key no kvcomm file gives is an error saying to log in to WeChat for Mac on this Mac, which writes them.
- A voice message is written as WeChat records it, SILK; converting it is not wxread's.
