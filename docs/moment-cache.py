# /// script
# requires-python = ">=3.12"
# dependencies = ["cryptography>=43"]
# ///
"""Counts how many of the Moments' files WeChat for Mac 4.x caches the rule
of docs/moment-cache.md names, on the account of this Mac.

    uv run docs/moment-cache.py --keys <file> [--account <dir>]

`--keys` is what `sudo wxread keys` answers; `--account` the account's
directory (the one above db_storage), else the only one under WeChat's data
directory. It decrypts sns/sns.db and its WAL in memory, as wxread does,
and prints one JSON object of counts: nothing of the account's content.
"""

import argparse
import glob
import hashlib
import json
import os
import sqlite3
import struct
import sys
import time
import xml.etree.ElementTree as ET
from pathlib import Path

from cryptography.hazmat.primitives.ciphers import Cipher, algorithms, modes

DATA_DIR = "~/Library/Containers/com.tencent.xinWeChat/Data/Documents/xwechat_files"
PAGE, RESERVE, SALT = 4096, 80, 16
# The copies of a Moment's image, by the suffix that names each.
IMAGE, THUMBNAIL, SMALL, VIDEO = 2, 1, 6, 3


def plain_database(db: bytes, wal: bytes, key: bytes) -> bytes:
    """The plain SQLite database `db` and its WAL hold under `key`: the WAL's
    committed frames laid over the pages, each page decrypted, in rollback
    mode."""
    pages = [db[i : i + PAGE] for i in range(0, len(db) - len(db) % PAGE, PAGE)]
    if len(wal) >= 32:
        salts = wal[16:24]
        pending, at = {}, 32
        while at + 24 + PAGE <= len(wal):
            number, commit = struct.unpack(">II", wal[at : at + 8])
            if wal[at + 8 : at + 16] != salts:
                break
            pending[number] = wal[at + 24 : at + 24 + PAGE]
            if commit:
                for n, page in pending.items():
                    pages.extend([b"\0" * PAGE] * (n - len(pages)))
                    pages[n - 1] = page
                del pages[commit:]
                pending = {}
            at += 24 + PAGE
    out = []
    for n, page in enumerate(pages, 1):
        start = SALT if n == 1 else 0
        iv = page[PAGE - RESERVE : PAGE - RESERVE + 16]
        dec = Cipher(algorithms.AES(key), modes.CBC(iv)).decryptor()
        body = dec.update(page[start : PAGE - RESERVE]) + dec.finalize()
        head = b"SQLite format 3\0" if n == 1 else b""
        out.append(head + body + page[PAGE - RESERVE :])
    plain = bytearray(b"".join(out))
    plain[18:20] = b"\x01\x01"
    return bytes(plain)


def name(post: str, media: str, copy: int) -> str:
    return hashlib.md5(f"{post}_{media}_{copy}".encode()).hexdigest()


def main() -> None:
    args = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    args.add_argument("--keys", required=True)
    args.add_argument("--account")
    a = args.parse_args()
    account = a.account
    if account is None:
        found = [os.path.dirname(d) for d in glob.glob(os.path.expanduser(f"{DATA_DIR}/*/db_storage"))]
        if len(found) != 1:
            sys.exit(f"pass --account <dir>: {len(found)} accounts under {DATA_DIR}")
        account = found[0]
    keys = json.loads(Path(a.keys).read_text())
    if "sns/sns.db" not in keys:
        sys.exit(f"{a.keys} holds no key of sns/sns.db: run `sudo wxread keys` again")
    db_path = Path(account, "db_storage/sns/sns.db")
    wal_path = db_path.with_name("sns.db-wal")
    db = db_path.read_bytes()
    wal = wal_path.read_bytes() if wal_path.exists() else b""
    conn = sqlite3.connect(":memory:")
    conn.deserialize(plain_database(db, wal, bytes.fromhex(keys["sns/sns.db"]["enc_key"])))

    # Every file cached under cache/<YYYY-MM>/Sns, by its name and month.
    cached = {}
    for path in glob.glob(os.path.join(account, "cache/*/Sns/*/*/*")):
        parts = path.split(os.sep)
        shelf, base = parts[-3], parts[-2] + parts[-1]
        if shelf == "Img" or (shelf == "Video" and base.endswith(".mp4")):
            cached[(shelf, base.removesuffix(".mp4"))] = parts[-5]

    # Each cached file the rule names, with whether it is in its post's month.
    named = {}

    def held(shelf: str, pid: str, mid: str, copy: int, month: str) -> bool:
        k = (shelf, name(pid, mid, copy))
        if k in cached:
            named[k] = cached[k] == month
        return k in cached

    posts = images = videos = 0
    copies = {IMAGE: 0, THUMBNAIL: 0, SMALL: 0}
    images_held = thumbnails_held = videos_held = 0
    for (content,) in conn.execute("SELECT content FROM SnsTimeLine"):
        try:
            post = ET.fromstring(content).find("TimelineObject")
        except (ET.ParseError, TypeError):
            continue
        if post is None:
            continue
        posts += 1
        pid = post.findtext("id")
        month = time.strftime("%Y-%m", time.localtime(int(post.findtext("createTime") or 0)))
        for media in post.iter("media"):
            mid, kind = media.findtext("id"), media.findtext("type")
            for copy in copies:
                copies[copy] += held("Img", pid, mid, copy, month)
            if kind == "2":
                images += 1
                images_held += held("Img", pid, mid, IMAGE, month)
                thumbnails_held += held("Img", pid, mid, THUMBNAIL, month)
            elif kind == "6":
                videos += 1
                videos_held += held("Video", pid, mid, VIDEO, month)

    img_files = sum(1 for shelf, _ in cached if shelf == "Img")
    mp4_files = len(cached) - img_files
    named_img = sum(1 for shelf, _ in named if shelf == "Img")
    print(
        json.dumps(
            {
                "posts": posts,
                "cached_image_files": img_files,
                "image_files_named": named_img,
                "image_files_named_by_copy": {f"_{c}": n for c, n in copies.items()},
                "post_images": images,
                "post_images_with_image_file": images_held,
                "post_images_with_thumbnail": thumbnails_held,
                "cached_video_files": mp4_files,
                "video_files_named": len(named) - named_img,
                "post_videos": videos,
                "post_videos_with_video_file": videos_held,
                "named_files_in_month_posted": sum(named.values()),
                "named_files": len(named),
            },
            indent=2,
        )
    )


if __name__ == "__main__":
    main()
