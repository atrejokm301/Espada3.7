import sqlite3
from pathlib import Path

base = Path(r"C:\Program Files (x86)\e-Sword")
files = []
for pat in ["*.bbli", "*.cmti", "*.dcti", "*.lexi"]:
    found = sorted(base.glob(pat))
    if not found:
        continue
    if pat == "*.bbli":
        prefer = [f for f in found if "1960" in f.name or "Reina" in f.name]
        files.append(prefer[0] if prefer else found[0])
    else:
        files.append(found[0])
        if len(found) > 1:
            files.append(found[1])

for f in files:
    print("===", f.name, "===")
    try:
        c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
        cur = c.cursor()
        cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
        tables = [r[0] for r in cur.fetchall()]
        print("tables:", tables)
        for t in tables:
            cur.execute(f"PRAGMA table_info([{t}])")
            cols = [r[1] for r in cur.fetchall()]
            print(f"  {t}: {cols}")
        if "Details" in tables:
            try:
                cur.execute("SELECT * FROM Details LIMIT 3")
                print("Details rows:", cur.fetchall())
            except Exception as e:
                print("Details err", e)
        if "Bible" in tables:
            cur.execute(
                "SELECT Book, Chapter, Verse, substr(Scripture,1,120) FROM Bible WHERE Book=1 AND Chapter=1 AND Verse=1"
            )
            print("Gen1:1", cur.fetchone())
            cur.execute("SELECT COUNT(*) FROM Bible WHERE Book=1 AND Chapter=1")
            print("Gen1 verse count", cur.fetchone())
        if "VerseCommentary" in tables:
            cur.execute(
                "SELECT Book, ChapterBegin, VerseBegin, substr(Comments,1,100) FROM VerseCommentary LIMIT 1"
            )
            print("vcomment", cur.fetchone())
        if "Dictionary" in tables:
            cur.execute("SELECT Topic, substr(Definition,1,100) FROM Dictionary LIMIT 2")
            print("dict", cur.fetchall())
        c.close()
    except Exception as e:
        print("FAIL", type(e).__name__, e)
    print()
