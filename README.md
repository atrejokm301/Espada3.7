# Asignación del Cielo Bible

App de estudio bíblico **offline** (Tauri 2 + UI vanilla en `ui/index.html`).

Tabs: **Biblia · Comentario · Diccionario · Léxico**

> **WinUI 3 preview** (branch `feature/winui3-shell`): same features via Acrylic + WebView2.  
> See [`src-winui/README.md`](src-winui/README.md) and run `.\scripts\run-winui.ps1`.

## Estado actual (Fase 2 — e-Sword real)

- Estructura Tauri + UI tema oscuro
- Lee módulos desde `C:\Program Files (x86)\e-Sword`
  - Biblias `.bbli` / `.bblx`
  - Comentarios `.cmti` / `.cmtx`
  - Diccionarios `.dcti` / `.dctx`
  - Léxicos `.lexi` / `.lexx`
- 66 libros, selector de módulo por pestaña
- Notas y marcadores en `localStorage`
- Default Biblia: Reina Valera 1960 si está instalada

## Requisitos

| Herramienta | Para qué |
|-------------|----------|
| **Node.js 18+** | Frontend (ya lo tenés) |
| **Rust** (rustup) | Compilar Tauri / `.exe` |
| **Microsoft C++ Build Tools** | Compilar en Windows |
| **WebView2** | Runtime de la ventana (suele venir en Win 10/11) |

### Instalar Rust (Windows)

1. Abrí PowerShell y ejecutá:

```powershell
winget install Rustlang.Rustup
```

O descargá el instalador: https://rustup.rs/

2. Cerrá y reabrí la terminal, luego:

```powershell
rustup default stable
rustc --version
cargo --version
```

3. Instalá **Visual Studio Build Tools** con la workload *Desktop development with C++*:
   - https://visualstudio.microsoft.com/visual-cpp-build-tools/

Docs oficiales Tauri: https://tauri.app/start/prerequisites/

## Desarrollo (solo frontend, sin Rust)

```powershell
cd C:\Users\kevtr\Proyectos\asignacion-del-cielo-bible
npm install
npm run dev
```

Abrí http://localhost:1420 en el navegador.

## Desarrollo con ventana de escritorio (Tauri)

```powershell
cd C:\Users\kevtr\Proyectos\asignacion-del-cielo-bible
npm install
npm run tauri dev
```

## Generar el `.exe` (instalador)

```powershell
cd C:\Users\kevtr\Proyectos\asignacion-del-cielo-bible
npm run tauri build
```

Salida típica:

- Instalador NSIS: `src-tauri\target\release\bundle\nsis\`
- MSI: `src-tauri\target\release\bundle\msi\`
- Binario: `src-tauri\target\release\Asignacion del Cielo Bible.exe`

## Plan de fases

1. **Estructura base** ← estás aquí  
2. **Módulos e-Sword** — leer `.bbli`, `.cmti`, `.dcti` de tu carpeta  
3. **Funciones** — full-text, comparación, notas/marcadores ampliados  
4. **IA local** — Ollama + disclaimer teológico  
5. **Compilar y probar** en tu PC  
6. **Pulir** — atajos, exportar estudios, tema  

## Comandos útiles para Grok

- *Integra mis módulos de e-Sword…*
- *Agrega más versos de Génesis completo…*
- *Implementa búsqueda full-text…*
- *Añade botón de IA local con Ollama…*
- *Compila el proyecto Tauri y dame el .exe…*

## Licencia de datos

Los textos bíblicos y comentarios de muestra se usan solo para desarrollo local.
Al integrar módulos e-Sword, respetá las licencias de cada recurso.
