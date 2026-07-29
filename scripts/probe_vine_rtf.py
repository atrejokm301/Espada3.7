import sqlite3, os, re

es = r"C:\Program Files (x86)\e-Sword"
for f in os.listdir(es):
    fl = f.lower()
    if not (fl.endswith(".dcti") or fl.endswith(".lexi") or fl.endswith(".dctx")):
        continue
    if "vine" not in fl and "expositivo" not in fl and "chavez" not in fl:
        continue
    p = os.path.join(es, f)
    print("===", f)
    conn = sqlite3.connect(f"file:{p}?mode=ro", uri=True)
    tables = [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")]
    print("tables", tables)
    table = "Dictionary" if "Dictionary" in tables else ("Lexicon" if "Lexicon" in tables else None)
    if not table:
        conn.close()
        continue
    # Find H2691 or atrio / hatser
    for term in ["H2691", "2691", "ATRIO", "atrio", "HATSER", "JATSER"]:
        rows = conn.execute(
            f"SELECT Topic FROM [{table}] WHERE Topic LIKE ? LIMIT 5", (f"%{term}%",)
        ).fetchall()
        if rows:
            print("topics", term, rows)
    # any definition containing H2691 or Gén_25
    for pat in ["%H2691%", "%Gén_25%", "%\\\\'e7%", "%jatser%"]:
        try:
            row = conn.execute(
                f"SELECT Topic, Definition FROM [{table}] WHERE typeof(Definition)='text' AND Definition LIKE ? LIMIT 1",
                (pat,),
            ).fetchone()
            if row:
                print("PAT", pat, "TOPIC", row[0])
                print(repr(row[1][:500]))
                print("---")
        except Exception as e:
            print("err", e)
    # blob?
    row = conn.execute(
        f"SELECT Topic, typeof(Definition), length(Definition) FROM [{table}] LIMIT 3"
    ).fetchall()
    print("samples", row)
    conn.close()
    print()
