import sqlite3
import zlib
from pathlib import Path

base = Path(r"C:\Program Files (x86)\e-Sword")

for f in list(base.glob("*.cmti")) + list(base.glob("*.cmtx")):
    try:
        c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
        cur = c.cursor()
        try:
            cur.execute("SELECT Title, Abbreviation FROM Details LIMIT 1")
            row = cur.fetchone()
        except Exception:
            row = (f.name, "")
        title = (row[0] or "") + " " + (row[1] or "")
        if not any(
            x in title.lower() or x in f.name.lower()
            for x in ["meyer", "day by day", "through the bible", "fb "]
        ):
            c.close()
            continue
        print("===", f.name, "===")
        print("Details:", row)
        cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
        tables = [r[0] for r in cur.fetchall()]
        print("tables:", tables)
        for tbl in tables:
            if "omment" in tbl or tbl == "Details":
                cur.execute(f"PRAGMA table_info([{tbl}])")
                print(" ", tbl, [r[1] for r in cur.fetchall()])
        for tbl in ["VerseCommentary", "ChapterCommentary", "BookCommentary"]:
            if tbl not in tables:
                continue
            cur.execute(f"SELECT COUNT(*) FROM [{tbl}]")
            print(f" {tbl} count", cur.fetchone()[0])
            cur.execute(f"SELECT * FROM [{tbl}] LIMIT 3")
            cols = [d[0] for d in cur.description]
            print("  cols", cols)
            for r in cur.fetchall():
                for i, col in enumerate(cols):
                    val = r[i]
                    if col.lower() in ("comments", "comment", "content", "data"):
                        print(
                            f"  {col}: type={type(val).__name__} "
                            f"len={len(val) if hasattr(val,'__len__') else '?'}"
                        )
                        if isinstance(val, (bytes, bytearray)):
                            b = bytes(val)
                            print("   first20 hex:", b[:20].hex())
                            print("   first20:", b[:20])
                            # try zlib
                            for wbits in (15, -15, 31, 47):
                                try:
                                    d = zlib.decompress(b, wbits)
                                    print(
                                        f"   zlib wbits={wbits} OK len={len(d)} "
                                        f"preview={d[:120]!r}"
                                    )
                                    break
                                except Exception as e:
                                    pass
                            # try skip header bytes
                            for skip in (0, 1, 2, 4, 8, 16):
                                try:
                                    d = zlib.decompress(b[skip:])
                                    print(f"   zlib skip={skip} OK", d[:100])
                                    break
                                except Exception:
                                    pass
                        elif isinstance(val, str):
                            print("   text preview:", val[:120])
        c.close()
    except Exception as e:
        print("ERR", f, e)
