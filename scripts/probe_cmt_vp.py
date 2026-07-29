import sqlite3, os

es = r"C:\Program Files (x86)\e-Sword"
for f in os.listdir(es):
    if not f.lower().endswith((".cmti", ".cmtx")):
        continue
    fl = f.lower()
    if not ("vida" in fl or "plena" in fl or fl.startswith("vp") or "estudio" in fl):
        continue
    print("===", f)
    p = os.path.join(es, f)
    conn = sqlite3.connect(f"file:{p}?mode=ro", uri=True)
    tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")]
    print("tables", tables)
    # Jer = 24
    if "VerseCommentary" in tables:
        cols = [r[1] for r in conn.execute("PRAGMA table_info(VerseCommentary)")]
        print("VC cols", cols)
        has_end = "ChapterEnd" in cols and "VerseEnd" in cols
        if has_end:
            rows = conn.execute(
                "SELECT ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, length(Comments) "
                "FROM VerseCommentary WHERE Book=24 AND ChapterBegin<=26 AND ChapterEnd>=26 "
                "ORDER BY ChapterBegin, VerseBegin LIMIT 40"
            ).fetchall()
            print("VC range covering ch26", rows)
            rows2 = conn.execute(
                "SELECT ChapterBegin, VerseBegin, ChapterEnd, VerseEnd, length(Comments) "
                "FROM VerseCommentary WHERE Book=24 AND ChapterBegin=26 "
                "ORDER BY VerseBegin LIMIT 40"
            ).fetchall()
            print("VC begin=26", rows2)
        else:
            rows = conn.execute(
                "SELECT ChapterBegin, VerseBegin, length(Comments) "
                "FROM VerseCommentary WHERE Book=24 AND ChapterBegin=26 "
                "ORDER BY VerseBegin LIMIT 40"
            ).fetchall()
            print("VC exact ch26", rows)
    if "ChapterCommentary" in tables:
        cols = [r[1] for r in conn.execute("PRAGMA table_info(ChapterCommentary)")]
        print("CC cols", cols)
        try:
            if "Chapter" in cols:
                rows = conn.execute(
                    "SELECT Chapter, length(Comments) FROM ChapterCommentary WHERE Book=24 AND Chapter=26"
                ).fetchall()
            else:
                rows = conn.execute(
                    "SELECT ChapterBegin, length(Comments) FROM ChapterCommentary WHERE Book=24 AND ChapterBegin=26"
                ).fetchall()
            print("CC ch26", rows)
        except Exception as e:
            print("CC err", e)
    if "BookCommentary" in tables:
        rows = conn.execute(
            "SELECT Book, length(Comments) FROM BookCommentary WHERE Book=24"
        ).fetchall()
        print("BC book24", rows)
    conn.close()
    print()
