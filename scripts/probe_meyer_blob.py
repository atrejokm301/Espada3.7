import sqlite3
import zlib

p = r"C:\Program Files (x86)\e-Sword\meyer.cmtx"
c = sqlite3.connect(f"file:{p}?mode=ro", uri=True)
b = c.execute(
    "SELECT Comments FROM VerseCommentary WHERE typeof(Comments)='blob' LIMIT 1"
).fetchone()[0]
print("len", len(b), "mod16", len(b) % 16)
print("hex32", b[:32].hex())
for off in range(0, min(48, len(b))):
    for wbits in (15, -15, 31):
        try:
            d = zlib.decompress(b[off:], wbits)
            print("zlib", off, wbits, "out", len(d), d[:100])
        except Exception:
            pass
# Compare two rows - same key?
rows = c.execute(
    "SELECT Comments FROM VerseCommentary WHERE typeof(Comments)='blob' LIMIT 5"
).fetchall()
for i, r in enumerate(rows):
    raw = r[0]
    print(i, "len", len(raw), "head", raw[:8].hex())
c.close()
