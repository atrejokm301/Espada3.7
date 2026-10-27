# Cross-reference data for Espada3.7

## Recommended file (already generated)

**`openbible_cross_references.sqlite`**

| Property | Value |
|----------|--------|
| Source | [OpenBible.info Labs — Cross References](https://www.openbible.info/labs/cross-references/) |
| License | [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/) |
| Rows | ~344,789 verse-to-verse links |
| Primary origin | Treasury of Scripture Knowledge (public domain), community-voted |
| Download raw TXT | https://a.openbible.info/data/cross-references.zip |

### Where to put it

Copy (or symlink) this file into your e-Sword modules folder, e.g.:

```
~/Downloads/for mac espada/openbible_cross_references.sqlite
```

The app auto-detects these names in the modules directory:

- `openbible_cross_references.sqlite`
- `openbible_cross_references.db`
- `cross_references.sqlite`
- any `*cross*` / `*openbible*` / `*xref*` `.sqlite`/`.db`

### Schema

```sql
CREATE TABLE cross_references (
  from_book INTEGER NOT NULL,      -- 1–66 (e-Sword / Protestant order)
  from_chapter INTEGER NOT NULL,
  from_verse INTEGER NOT NULL,
  to_book INTEGER NOT NULL,
  to_chapter INTEGER NOT NULL,
  to_verse_start INTEGER NOT NULL,
  to_verse_end INTEGER NOT NULL,
  votes INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_cr_from ON cross_references(from_book, from_chapter, from_verse);
```

## Alternative: e-Sword TSK module

| File | Works? | Notes |
|------|--------|-------|
| **`TSKe.cmti` / `tsk.cmti`** (modern, unencrypted) | ✅ Yes | Verse commentary rows with dense `<ref>` links |
| **`tsk.cmtx`** (legacy) | ❌ No | DRM-encrypted; Espada never decrypts |

If you already have `tsk.cmtx` in the modules folder, the UI will list it as 🔒 and fall back to OpenBible SQLite when present.

Do **not** use `.refi` study-Bible note modules for this button — those are topical/book notes, not verse cross-reference graphs.

## Attribution

When redistributing OpenBible data, keep the CC BY attribution to OpenBible.info.
