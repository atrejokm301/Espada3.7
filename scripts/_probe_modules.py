import sqlite3
from pathlib import Path

base = Path(r"C:\Program Files (x86)\e-Sword")


def open_ro(f):
    return sqlite3.connect(f"file:{f}?mode=ro", uri=True)


print("=== COMMENTARIES Gen 1:1 ===")
for f in sorted(base.glob("*.cmti"))[:20]:
    c = open_ro(f)
    cur = c.cursor()
    try:
        cur.execute("SELECT Abbreviation FROM Details LIMIT 1")
        abbr = cur.fetchone()[0]
    except Exception:
        abbr = f.name
    n = 0
    typ = "?"
    try:
        cur.execute(
            """SELECT COUNT(*), typeof(Comments) FROM VerseCommentary
               WHERE Book=1 AND ChapterBegin<=1 AND ChapterEnd>=1
               AND NOT (ChapterBegin=1 AND VerseBegin>1)
               AND NOT (ChapterEnd=1 AND VerseEnd<1)"""
        )
        row = cur.fetchone()
        n, typ = row[0], row[1] if row else (0, "?")
    except Exception:
        try:
            cur.execute(
                "SELECT COUNT(*), typeof(Comments) FROM VerseCommentary WHERE Book=1 AND ChapterBegin=1 AND VerseBegin=1"
            )
            row = cur.fetchone()
            n, typ = row[0], row[1] if row else (0, "?")
        except Exception as e:
            n, typ = 0, str(e)
    # sample text length
    preview = ""
    try:
        cur.execute(
            """SELECT Comments FROM VerseCommentary
               WHERE Book=1 AND ChapterBegin<=1 AND ChapterEnd>=1
               AND NOT (ChapterBegin=1 AND VerseBegin>1)
               AND NOT (ChapterEnd=1 AND VerseEnd<1) LIMIT 1"""
        )
        val = cur.fetchone()
        if val and val[0] is not None:
            v = val[0]
            if isinstance(v, bytes):
                preview = f"BYTES len={len(v)} head={v[:8].hex()}"
            else:
                preview = str(v)[:80].replace("\n", " ")
    except Exception as e:
        preview = str(e)
    print(f"{abbr:20} n={n} type={typ} | {preview}")
    c.close()

print("\n=== DICTIONARIES term Dios/crear/H1254 ===")
for f in sorted(base.glob("*.dcti"))[:12]:
    c = open_ro(f)
    cur = c.cursor()
    try:
        cur.execute("SELECT Abbreviation FROM Details LIMIT 1")
        abbr = cur.fetchone()[0]
    except Exception:
        abbr = f.stem[:20]
    cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = [r[0] for r in cur.fetchall()]
    samples = []
    if "Dictionary" in tables:
        for q in ("%Dios%", "%crear%", "%luz%", "H1254%", "G26%"):
            try:
                cur.execute(
                    "SELECT Topic FROM Dictionary WHERE Topic LIKE ? LIMIT 2", (q,)
                )
                samples.extend([r[0] for r in cur.fetchall()])
            except Exception:
                pass
    print(f"{abbr:20} tables={tables} hits={samples[:5]}")
    c.close()

print("\n=== LEXICONS H1254 / G26 ===")
for f in sorted(base.glob("*.lexi")):
    c = open_ro(f)
    cur = c.cursor()
    try:
        cur.execute("SELECT Abbreviation, Title FROM Details LIMIT 1")
        abbr, title = cur.fetchone()
    except Exception:
        abbr, title = f.stem[:15], ""
    cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = [r[0] for r in cur.fetchall()]
    hits = []
    tbl = "Lexicon" if "Lexicon" in tables else ("Dictionary" if "Dictionary" in tables else None)
    if tbl:
        for q in ("H1254", "H1254%", "G26", "G26%", "H1"):
            try:
                cur.execute(f"SELECT Topic FROM [{tbl}] WHERE Topic LIKE ? LIMIT 2", (q,))
                hits.extend([r[0] for r in cur.fetchall()])
            except Exception as e:
                hits.append(f"err:{e}")
        # first topics
        try:
            cur.execute(f"SELECT Topic FROM [{tbl}] LIMIT 5")
            first = [r[0] for r in cur.fetchall()]
        except Exception as e:
            first = [str(e)]
    else:
        first = []
    print(f"{abbr:15} hits={hits[:4]} first={first}")
    c.close()
