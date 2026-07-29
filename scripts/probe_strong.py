import sqlite3
import os
import re

es = r"C:\Program Files (x86)\e-Sword"

def open_ro(path):
    return sqlite3.connect(f"file:{path}?mode=ro", uri=True)

# --- Interlinear ---
p = os.path.join(es, "00Interlineal-iRV 1960+.bbli")
print("INTER exists", os.path.exists(p))
if os.path.exists(p):
    conn = open_ro(p)
    print("tables", [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")])
    try:
        print("cols", [r[1] for r in conn.execute("PRAGMA table_info(Bible)")])
        row = conn.execute(
            "SELECT Scripture FROM Bible WHERE Book=40 AND Chapter=5 AND Verse=12"
        ).fetchone()
        if row:
            t = row[0]
            if isinstance(t, bytes):
                print("type bytes len", len(t), "head", t[:120])
                # try decode
                for enc in ("utf-8", "latin-1", "utf-16"):
                    try:
                        s = t.decode(enc)
                        print("decoded", enc, s[:500])
                        break
                    except Exception:
                        pass
            else:
                print("type", type(t), str(t)[:900])
    except Exception as e:
        print("bible err", e)
    conn.close()

# --- Concordance ---
p2 = os.path.join(es, "00_NC-STRONG-E_Nueva_Concordancia_Strong_Exhaustiva.dcti")
print("\nCONC exists", os.path.exists(p2))
if os.path.exists(p2):
    conn = open_ro(p2)
    print("tables", [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")])
    try:
        for term in ["Gozaos", "gozaos", "gozar", "Gozo", "gozáos"]:
            rows = conn.execute(
                "SELECT Topic FROM Dictionary WHERE Topic LIKE ? LIMIT 8",
                (f"%{term}%",),
            ).fetchall()
            print("topic", term, rows)
        rows = conn.execute(
            "SELECT Topic, substr(cast(Definition as text),1,250) FROM Dictionary "
            "WHERE typeof(Definition)='text' AND Definition LIKE ? LIMIT 8",
            ("%Gozaos%",),
        ).fetchall()
        print("def Gozaos", rows)
        # sample topics
        rows = conn.execute("SELECT Topic FROM Dictionary LIMIT 15").fetchall()
        print("sample topics", rows)
    except Exception as e:
        print("conc err", e)
    conn.close()

# --- Strong lexicon sample ---
p3 = os.path.join(es, "01 Diccionario strong.lexi")
print("\nSTRONG LEX", os.path.exists(p3))
if os.path.exists(p3):
    conn = open_ro(p3)
    print("tables", [r[0] for r in conn.execute("SELECT name FROM sqlite_master WHERE type='table'")])
    for key in ["G5463", "5463", "G5463 ", "chairo"]:
        rows = conn.execute(
            "SELECT Topic FROM Lexicon WHERE Topic LIKE ? LIMIT 5", (f"{key}%",)
        ).fetchall() if True else []
        try:
            rows = conn.execute(
                "SELECT Topic FROM Lexicon WHERE Topic = ? OR Topic LIKE ? LIMIT 5",
                (key, f"{key}%"),
            ).fetchall()
        except Exception:
            try:
                rows = conn.execute(
                    "SELECT Topic FROM Dictionary WHERE Topic = ? OR Topic LIKE ? LIMIT 5",
                    (key, f"{key}%"),
                ).fetchall()
            except Exception as e:
                rows = [str(e)]
        print("lex", key, rows)
    conn.close()

# RV1960 verse for reference
for name in os.listdir(es):
    if "1960" in name.lower() and name.lower().endswith(".bbli") and "inter" not in name.lower():
        print("\nRV candidate", name)
        break
