import sqlite3
from pathlib import Path
base = Path(r"C:\Program Files (x86)\e-Sword")
f = base / "01 Diccionario de hebreo biblico Chavez.lexi"
c = sqlite3.connect(f"file:{f}?mode=ro", uri=True)
cur = c.cursor()
for pat in ["%&#x05D1;&#x05E8;&#x05D0;%", "%&#x05d1;&#x05e8;&#x05d0;%"]:
    cur.execute(
        "SELECT Topic FROM Lexicon WHERE typeof(Definition)='text' AND Definition LIKE ? LIMIT 10",
        (pat,),
    )
    print(pat, "->", cur.fetchall())
c.close()
