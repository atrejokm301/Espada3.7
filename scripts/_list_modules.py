import sqlite3
from pathlib import Path

base = Path(r"C:\Program Files (x86)\e-Sword")

print("=== BIBLES ===")
for f in sorted(base.glob("*.bbli")) + sorted(base.glob("*.bblx")):
    try:
        c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
        cur = c.cursor()
        cur.execute("SELECT Title, Abbreviation FROM Details LIMIT 1")
        row = cur.fetchone()
        print(f"{f.name} -> {row}")
        c.close()
    except Exception as e:
        print(f"{f.name} ERR {e}")

print("\n=== LEXI ===")
for f in sorted(base.glob("*.lexi")):
    try:
        c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
        cur = c.cursor()
        cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
        tables = [r[0] for r in cur.fetchall()]
        print(f.name, tables)
        for t in tables:
            if t.lower() in ("dictionary", "lexicon", "topic"):
                cur.execute(f"PRAGMA table_info([{t}])")
                print(" ", t, [r[1] for r in cur.fetchall()])
                cur.execute(f"SELECT * FROM [{t}] LIMIT 1")
                print("  sample", str(cur.fetchone())[:150])
        c.close()
    except Exception as e:
        print(f.name, e)

print("\n=== CMTI sample titles ===")
for f in sorted(base.glob("*.cmti"))[:15]:
    try:
        c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
        cur = c.cursor()
        cur.execute("SELECT Title, Abbreviation FROM Details LIMIT 1")
        print(f.name, "->", cur.fetchone())
        c.close()
    except Exception as e:
        print(f.name, e)
