# Asignación del Cielo — WinUI 3 shell (preview)

Branch experiment: host the existing `ui/index.html` study UI inside a **WinUI 3** window with:

- **Desktop Acrylic** system backdrop (translucent Fluent glass)
- **WebView2** for the full feature UI (same methods as Tauri)
- **Rust `adc-api` sidecar** on `http://127.0.0.1:17865` — same command names as Tauri `invoke`

The classic Tauri app under `src-tauri` is untouched and remains the stable path.

## Prerequisites

| Tool | Notes |
|------|--------|
| **.NET 8 SDK** | `winget install Microsoft.DotNet.SDK.8` |
| **Rust** | same as Tauri (`cargo`) |
| **WebView2 Runtime** | usually preinstalled on Win 10/11 |
| **Windows 10 1809+** | Windows App SDK / WinUI 3 |

## Quick start

From the repo root:

```powershell
.\scripts\run-winui.ps1
```

Or step by step:

```powershell
# 1) e-Sword API (same Rust readers as Tauri)
cd src-tauri
cargo build --bin adc-api
# optional: cargo run --bin adc-api

# 2) WinUI host
cd ..\src-winui\AsignacionDelCielo.WinUI
$env:ADC_API_EXE = "..\..\src-tauri\target\debug\adc-api.exe"
$env:ADC_UI_DIR  = "..\..\ui"
dotnet run -c Debug -p:Platform=x64
```

## Architecture

```
┌─────────────────────────────────────────────┐
│  WinUI 3 window (DesktopAcrylicBackdrop)    │
│  ┌───────────────────────────────────────┐  │
│  │ WebView2  →  ui/index.html            │  │
│  │  invoke(cmd) → POST /invoke           │  │
│  └───────────────┬───────────────────────┘  │
└──────────────────┼──────────────────────────┘
                   │ 127.0.0.1:17865
                   ▼
            adc-api.exe  (Rust)
            esword/* modules
```

Commands (feature parity with Tauri):

- `list_esword_modules`, `list_bible_books`, `get_bible_chapter`
- `get_verse_commentaries`, `search_dictionary`, `search_lexicon`
- `search_commentary_term`, `suggest_dictionary_topics`
- `resolve_strongs`, `probe_modules_content`, `app_info`

## Theme

On first launch under WinUI, the UI prefers the **Fluent Glass** theme (translucent panels + blur). You can still switch themes from the in-app theme picker.

## Status

Scaffold / shell first. Next steps when you’re happy with the host:

1. Copy `adc-api.exe` + `ui/` next to the packaged WinUI binary
2. Optional native WinUI chrome (settings, about) outside WebView
3. Long-term: decide whether to keep hybrid WebView or port panels to XAML
