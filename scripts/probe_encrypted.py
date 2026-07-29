import os, sqlite3

es = r"C:\Program Files (x86)\e-Sword"
for f in sorted(os.listdir(es)):
    if not f.lower().endswith((".cmti", ".cmtx", ".dcti", ".dctx", ".bbli", ".lexi", ".lexx", ".bblx")):
        continue
    p = os.path.join(es, f)
    try:
        c = sqlite3.connect(f"file:{p}?mode=ro", uri=True)
    except Exception as e:
        print("OPEN_FAIL", f, e)
        continue
    tables = [r[0] for r in c.execute("SELECT name FROM sqlite_master WHERE type='table'")]
    for t, col in [
        ("Bible", "Scripture"),
        ("VerseCommentary", "Comments"),
        ("ChapterCommentary", "Comments"),
        ("Dictionary", "Definition"),
        ("Lexicon", "Definition"),
    ]:
        if t not in tables:
            continue
        try:
            row = c.execute(
                f"SELECT typeof([{col}]), length([{col}]) FROM [{t}] LIMIT 5"
            ).fetchall()
            blobish = [r for r in row if r[0] == "blob"]
            if not blobish:
                continue
            b = c.execute(
                f"SELECT [{col}] FROM [{t}] WHERE typeof([{col}])='blob' LIMIT 1"
            ).fetchone()
            if not b or not b[0]:
                continue
            raw = b[0]
            head = raw[:24]
            print(
                f"{f} | {t}.{col} | len={len(raw)} | hex={head.hex()} | "
                f"zlib={raw[0]==0x78} gzip={raw[:2]==bytes([0x1f,0x8b])}"
            )
        except Exception as e:
            print("ERR", f, t, e)
    c.close()
