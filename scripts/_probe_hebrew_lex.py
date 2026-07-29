import sqlite3
import re
from pathlib import Path

base = Path(r"C:\Program Files (x86)\e-Sword")

# Find Chávez and Strong lexicons
lex_files = list(base.glob("*.lexi"))
for f in lex_files:
    c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
    cur = c.cursor()
    try:
        cur.execute("SELECT Title, Abbreviation FROM Details LIMIT 1")
        title, abbr = cur.fetchone()
    except Exception:
        title, abbr = f.name, f.stem
    print("\n===", f.name, "|", abbr, "|", title[:60] if title else "")

    cur.execute("SELECT name FROM sqlite_master WHERE type='table'")
    tables = [r[0] for r in cur.fetchall()]
    print("tables:", tables)
    tbl = "Lexicon" if "Lexicon" in tables else None
    if not tbl:
        c.close()
        continue

    # H1254 entry
    for q in ("H1254", "H01254", "1254"):
        cur.execute(f"SELECT Topic, typeof(Definition), length(Definition) FROM [{tbl}] WHERE Topic=?", (q,))
        row = cur.fetchone()
        if row:
            print(f"  exact Topic={q}: type={row[1]} len={row[2]}")
            cur.execute(f"SELECT Definition FROM [{tbl}] WHERE Topic=?", (q,))
            defn = cur.fetchone()[0]
            if isinstance(defn, bytes):
                print("  BLOB head", defn[:40].hex())
                try:
                    s = defn.decode("utf-8", errors="replace")
                except Exception:
                    s = str(defn[:200])
            else:
                s = str(defn)
            print("  def preview:", s[:300].replace("\n", " "))
            # does bare ברא appear?
            print("  contains ברא:", "ברא" in s)
            # any hebrew
            he = re.findall(r"[\u0590-\u05FF]+", s)
            print("  hebrew tokens sample:", he[:15])
            break

    # Search Topic containing hebrew
    try:
        cur.execute(
            f"SELECT Topic FROM [{tbl}] WHERE Topic LIKE ? LIMIT 10",
            ("%ברא%",),
        )
        print("  Topic LIKE %ברא%:", cur.fetchall())
    except Exception as e:
        print("  Topic LIKE err", e)

    # Definition text search
    try:
        cur.execute(
            f"SELECT Topic FROM [{tbl}] WHERE typeof(Definition)='text' AND Definition LIKE ? LIMIT 10",
            ("%ברא%",),
        )
        print("  Def text LIKE %ברא%:", cur.fetchall())
    except Exception as e:
        print("  Def text LIKE err", e)

    # count blob vs text
    cur.execute(f"SELECT typeof(Definition), COUNT(*) FROM [{tbl}] GROUP BY typeof(Definition)")
    print("  types:", cur.fetchall())

    # sample topics that look non-H/G
    cur.execute(f"SELECT Topic FROM [{tbl}] WHERE Topic NOT LIKE 'H%' AND Topic NOT LIKE 'G%' LIMIT 15")
    print("  non-HG topics:", cur.fetchall())

    c.close()
