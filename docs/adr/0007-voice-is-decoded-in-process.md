# A voice message is decoded in-process, by a vendored SILK decoder

`wxread voice` writes a voice message as a file that plays: WeChat keeps each one's sound as SILK v3 (`\x02#!SILK_V3…`) in `message/media_<n>.db`'s `VoiceInfo`, by its chat and server id, and no player plays SILK. wxread decodes it itself, in the process, with the SILK SDK's reference decoder ported to safe Rust (silk-decoder-rs 0.1.0, MIT), vendored as the crate `silk` as bizkernel vendored it: as it was but for its entry point, which read a path and answered a WAV, where wxread's takes the bytes and answers the samples. It is written as a WAV, 16-bit mono PCM at 24 kHz, the rate WeChat records at, which every player plays; `--raw` writes the SILK as WeChat keeps it, byte for byte. The WAV, the media databases' order and every refusal are the pure crate `wxread-voice`'s.

No other tool is run (ffmpeg, silk-v3-decoder, afconvert): an agent CLI is one binary that works where it is installed, and a tool it shells out to is one more thing missing on the next Mac. The crates.io release of the decoder is not used because it reads a path: the SILK is decrypted from a database (0003), and writing it to a temp file to be read back is the decrypted byte on disk that 0003 refuses.

## Consequences

- `silk` is a dependency kept in the workspace, not a pure crate: like ruzstd it is not in `verify.sh`'s `PURE`, Flux does not check it, and it keeps its own idiom and license (its `LICENSE`, MIT). Its tests hold it to the reference C decoder's samples for a synthetic tone, which the CLI's tests read too.
- A decoder that panics on bytes it was not written for is a voice that does not decode, answered as an error naming `--raw`, never a run that ends with no JSON.
- The `--out` file is the one decrypted thing wxread writes, and only where it is told: the rule of 0003 holds for everything else.
- Another output format (Opus, MP3) would need an encoder vendored or written the same way; WAV needs none.
