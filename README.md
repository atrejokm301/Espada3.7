# Espada 3.7 — Windows

App de estudio bíblico **nativa y offline for Windows** (Tauri 2 + UI en `ui/`).

> Product name: **Espada 3.7** · Platform: **Windows** · Branch: **`windows`**  
> Internal crate (for now): `asignacion-del-cielo-bible`

## Multi-platform layout (this repo)

| Branch | Platform | Stack |
|--------|----------|--------|
| **`main`** | macOS | Espada 3.7 for Apple Silicon (existing) |
| **`windows`** | Windows | This code — Tauri 2 + WebView2 + e-Sword |

Repo: [atrejokm301/Espada3.7](https://github.com/atrejokm301/Espada3.7)

Tabs: **Biblia · Comentario · Diccionario · Léxico**

Cross-references (release): en Biblia, selecciona versículo o palabra → **↔ Referencias**
- **Por versículo**: TSK / TSKe / Referencias RV (comentarios e-Sword), citas clicables
- **Por palabra**: concordancia en la Biblia actual (otras ocurrencias de la palabra)

## Producto principal: Tauri (nativo)

| Pieza | Rol |
|--------|-----|
| **`Start-ADC-Native.bat`** | Abrir la app (recomendado) |
| **`ui/`** | Interfaz de estudio (`index.html` + `app.js`) |
| **`src-tauri/`** | Shell nativo Windows + lectores e-Sword (IPC) |
| **WebView2** | Dibuja la UI dentro del `.exe` (sin Edge como host) |

Todo lo nuevo de la UI vive en **`ui/`** y se **empaqueta en el build de Tauri** (`frontendDist: ../ui`).  
No hace falta `adc-api` ni Edge para el uso diario.

```powershell
cd C:\Users\kevtr\Proyectos\asignacion-del-cielo-bible
.\Start-ADC-Native.bat
# o:
npm run native
# o (hot reload del shell):
npm run tauri:dev
```

### Generar instalador / release

```powershell
npm run tauri:build
# o: cargo tauri build  (desde repo con CLI)
```

Salida típica:

- Instalador NSIS: `src-tauri\target\release\bundle\nsis\`
- MSI: `src-tauri\target\release\bundle\msi\`
- Binario: `src-tauri\target\release\Espada 3.7.exe` (product name) / `asignacion-del-cielo-bible.exe`

## Estado (e-Sword local)

- Lee módulos desde `C:\Program Files (x86)\e-Sword`
  - Biblias `.bbli` / `.bblx`
  - Comentarios `.cmti` / `.cmtx`
  - Diccionarios `.dcti` / `.dctx`
  - Léxicos `.lexi` / `.lexx`
- 66 libros, selector de módulo por pestaña
- Notas y marcadores en `localStorage`
- Default Biblia: Reina Valera 1960 si está instalada
- **Offline**: textos y API son locales; fuentes web son opcionales (cae a Segoe UI)

## Requisitos

| Herramienta | Para qué |
|-------------|----------|
| **Rust** (rustup) | Compilar Tauri / `.exe` |
| **Microsoft C++ Build Tools** | Compilar en Windows |
| **WebView2** | Runtime de la ventana (suele venir en Win 10/11) |
| **Node.js 18+** | Solo CLI de Tauri (`npm run tauri:*`) — opcional si usás el `.bat` + cargo |

### Instalar Rust (Windows)

```powershell
winget install Rustlang.Rustup
# reiniciar terminal
rustup default stable
```

Instalá **Visual Studio Build Tools** con *Desktop development with C++*:  
https://visualstudio.microsoft.com/visual-cpp-build-tools/

Docs Tauri: https://tauri.app/start/prerequisites/

## Shells alternativos (no primarios)

| Launcher | Qué es |
|----------|--------|
| **`Start-ADC-Stable.bat`** | Fallback: Edge app + `adc-api` HTTP (depuración) |
| **`src-winui/`** | Preview WinUI 3 + Acrylic (experimental; ver `src-winui/README.md`) |

La UI en `ui/` detecta el host (Tauri IPC primero, luego HTTP).

## Arquitectura

```
┌─────────────────────────────────────┐
│  asignacion-del-cielo-bible.exe     │  ← nativo (Tauri)
│  ┌───────────────────────────────┐  │
│  │  ui/ (HTML + app.js)          │  │
│  │  invoke("get_bible_chapter")  │  │
│  └─────────────┬─────────────────┘  │
│                │ IPC local            │
│  ┌─────────────▼─────────────────┐  │
│  │  Rust e-Sword readers         │  │
│  └─────────────┬─────────────────┘  │
└────────────────┼────────────────────┘
                 ▼
     C:\Program Files (x86)\e-Sword
```

## Licencia de datos

Al integrar módulos e-Sword, respetá las licencias de cada recurso.
