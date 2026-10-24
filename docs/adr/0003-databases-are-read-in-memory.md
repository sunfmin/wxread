# Databases are decrypted into memory, their committed WAL frames laid over them

A command reads one of an account's databases by reading the file and the `-wal` beside it, checking the key against the first page's HMAC (and the salt the keys say, when they say one) before any byte is decrypted, decrypting every page into memory, laying over them the frames SQLite itself would read from the WAL (those of its current generation whose checksums hold, through the last frame that ends a commit), and opening the result read-only from memory with `sqlite3_deserialize`, its sorting kept in memory too. No decrypted byte is written to disk, not to a temp file and not beside WeChat's: the keys open the whole of a person's chats, and a decrypted copy left on disk would outlive the run that made it. The WAL is read rather than left out because WeChat checkpoints rarely, so what was said last is often only there.

The page format (AES-256-CBC, an IV and an HMAC-SHA512 in each page's last 80 bytes, the HMAC key PBKDF2-derived from the key and the salt XOR 0x3a) is as wechat-cli and wechat-decrypt found it, and as bizkernel's wechat-intake reads it; it lives in the pure crate `wxread-sqlcipher`, which is checked by Flux like every pure crate (0002).

## Consequences

- Every run decrypts again what it reads: a command costs a read of the databases it names, and nothing is cached between runs.
- A key that no longer opens its database is refused before anything is read, naming the database and the fix (`sudo wxread keys`, then supply them again), never read as garbage.
- A database larger than memory cannot be read; WeChat's are megabytes to a few hundred.
- The tests' synthetic accounts are made in memory too, from `wxread-schema`'s declarations, encrypted with `wxread_sqlcipher::encrypt` and given a WAL by `wxread_sqlcipher::wal`, so a test that no decrypted byte reaches the disk checks wxread alone.
