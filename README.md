# Espada3.7 — Asignación del Cielo Bible (macOS)

Aplicación de estudio bíblico **offline** para macOS. Lee módulos compatibles con **e-Sword** (SQLite: `.bbli`, `.cmti`, `.dcti`, `.lexi`, etc.) desde una carpeta local.

Estilo visual **Liquid Glass** (paneles translúcidos, tipografía del sistema, temas claro/oscuro). Interfaz en **español**.

> Producto: equivalente macOS de *Asignación del Cielo Bible* (Windows).  
> **Sin IA / Ollama / nube.** Todo es local.

---

## Requisitos

- macOS 11+
- [Rust](https://rustup.rs/) (stable)
- Xcode Command Line Tools (`xcode-select --install`)
- Node.js 18+ (solo para el CLI de Tauri vía npm, opcional si usa `cargo tauri`)

---

## Compilar y ejecutar

```bash
cd ~/Espada3.7

# Opción A — cargo tauri (recomendado)
source "$HOME/.cargo/env"
cargo install tauri-cli --version "^2"   # una vez
cd src-tauri
cargo tauri dev     # desarrollo
cargo tauri build   # genera .app en target/release/bundle/macos/
```

```bash
# Release optimizado (O3, thin LTO, -march=native, CFLAGS agresivos)
./build-release.sh

# Con PGO de dos pasos (instrumentar → usar app → recompilar):
./build-release.sh --pgo
```

```bash
# Opción B — npm scripts
cd ~/Espada3.7
npm install
npm run dev
npm run build
```

La app empaquetada queda en:

`src-tauri/target/release/bundle/macos/Espada3.7.app`

### Optimización de release (mapa de flags)

| Pedido (C/C++) | En este proyecto (Rust / LLVM / C deps) |
|----------------|----------------------------------------|
| `-O3` | `opt-level = 3` |
| ThinLTO | `lto = "thin"` |
| `-march=native` | `RUSTFLAGS=-C target-cpu=native` + `CFLAGS=-march=native` |
| `-fstrict-aliasing` / `-ffast-math` | en `CFLAGS` para SQLite embebido |
| `-fno-exceptions` / `-fno-rtti` | Rust `panic = "abort"`; flags en C/C++ deps |
| PGO `-fprofile-generate` / `-fprofile-use` | `./build-release.sh --pgo` |
| BOLT / AutoFDO | documentados en el script (requieren herramientas extra) |
| Bitcode App Thinning | deprecado en macOS moderno; no se fuerza en el bundle final |

**Nota:** un binario `native` está afinado para **esta** Mac; no lo redistribuya a CPUs distintas sin recompilar.

---

## Carpeta de módulos

En el primer arranque (o en **⚙ Ajustes**) elija la carpeta que contiene los módulos e-Sword.

### En este Mac

Carpeta recomendada (por defecto):

`~/Downloads/for mac espada`

Otras ubicaciones habituales (si existen):

- `~/Documents/e-Sword`
- `~/Library/Application Support/e-Sword`
- `~/e-Sword`
- `~/Desktop`

### Desde Windows

La ruta típica de e-Sword en Windows es:

`C:\Program Files (x86)\e-Sword`

Copie los archivos de módulo a una carpeta en el Mac (p. ej. `~/Desktop` o `~/Documents/e-Sword`) y selecciónela en Ajustes.

| Extensión | Tipo        | Notas                                      |
|-----------|-------------|--------------------------------------------|
| `.bbli`   | Biblia      | Preferida (moderna, sin DRM)               |
| `.cmti`   | Comentario  | Preferida                                  |
| `.dcti`   | Diccionario | Preferida                                  |
| `.lexi`   | Léxico      | Preferida                                  |
| `.refi`   | Referencia  | Listado (notas temáticas; no es el grafo CR) |
| `openbible_cross_references.sqlite` | Referencias cruzadas | ~345k enlaces (OpenBible / CC BY); ver `data/README.md` |
| `.bblx` / `.cmtx` / `.dctx` / `.lexx` | Legado | Pueden ir **cifrados** → se marcan 🔒 y no se descifran |

### Referencias cruzadas (botón en la barra de lectura)

1. Coloque `openbible_cross_references.sqlite` en la carpeta de módulos  
   (copia lista en `Espada3.7/data/` y en `~/Downloads/for mac espada/`).
2. Opcional: un módulo **TSK / TSKe en `.cmti` sin DRM**.  
   **`tsk.cmtx` cifrado no funciona** (Espada no descifra e-Sword DRM).
3. En la pestaña Biblia, pulse el botón **⇄** junto al título del capítulo.

**No se reverse-enginea DRM.** Use módulos modernos no cifrados cuando sea posible.

---

## Funciones (paridad con Windows)

| Área | Estado |
|------|--------|
| Lectura de Biblia + selectores libro/capítulo/versículo | ✅ |
| Prev/sig. capítulo, modal de versículos | ✅ |
| Persistencia de posición (localStorage) | ✅ |
| Historial / Recientes (~40) | ✅ |
| Referencias cruzadas (botón ⇄ · OpenBible SQLite / TSK `.cmti`) | ✅ |
| Paralela: primaria + hasta 2 | ✅ |
| Palabras de Cristo en rojo (`<red>`, fuentes rojas) | ✅ |
| Comentario con filtros Versículo / Capítulo / Libro | ✅ |
| Diccionario por **tema/lema** (no spam del cuerpo) | ✅ |
| Léxico Strong’s (multi-módulo) | ✅ |
| Interlinking: refs bíblicas + Strong’s + hebreo/griego | ✅ |
| Tooltips suaves + paneles in-app (sin `window.open`) | ✅ |
| “Ir a pasaje” / “Abrir en Léxico” | ✅ |
| Notas personales por capítulo | ✅ |
| Resaltados semánticos (gold, sky, rose, lime, violet, peach) | ✅ |
| Subrayado + menú contextual (Copiar, Compartir, …) | ✅ |
| Temas: True Dark, Plain White, Pastel Green, Pastel Yellow, Light Blue | ✅ |
| Liquid Glass + `prefers-reduced-transparency` | ✅ |
| Caché de módulos (mtime/size) y de capítulos | ✅ |
| Módulos cifrados etiquetados 🔒 | ✅ |
| AI / Ollama / Gemma | ❌ no incluido |

### Límites solo-macOS

- No hay instalador e-Sword por defecto en Mac: hay que **aportar** la carpeta de módulos.
- Compartir usa la API web del WebView (puede variar según macOS).
- Vibrancy nativa del sistema no se fuerza de forma agresiva para no dañar la legibilidad del texto bíblico; el cristal es CSS (`backdrop-filter`).

---

## Permisos

- Lectura de la carpeta de módulos que el usuario elija (diálogo nativo).
- No requiere red para el estudio.
- Al empaquetar, macOS puede pedir permitir la app si no está firmada/notarizada (`clic derecho → Abrir` la primera vez).

---

## Estructura del proyecto

```
Espada3.7/
  ui/                 # HTML/CSS/JS (vanilla)
  src-tauri/
    src/
      main.rs
      lib.rs          # comandos Tauri
      modules.rs      # scan + SQLite e-Sword
      text.rs         # limpieza HTML/RTF
      books.rs        # libros ES + alias
    tauri.conf.json
  README.md
  package.json
```

---

## Nombre del producto

- **App / proyecto:** Espada3.7  
- **Título largo:** Asignación del Cielo Bible (macOS)
