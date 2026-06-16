# daz-sailor

daz-sailor sorts DAZ Studio downloads into the right place automatically. Drop `.zip`, `.rar`, or `.7z` files into your downloads folder and the tool will either:

- **Queue them for DAZ Install Manager (DIM)** — when the archive contains a DIM package (`manifest.dsx` + `supplement.dsx`), or
- **Install directly into your content library** — when it finds recognizable DAZ library folders (`People`, `data`, `Runtime`, etc.)

It also handles nested archives (for example, a `.zip` inside a `.rar`) and moves processed archives into a `done` subfolder.

---

## Prerequisites

### Rust

Install Rust from [https://rustup.rs](https://rustup.rs). After installing, open a new terminal and confirm:

```powershell
rustc --version
cargo --version
```

### Windows: C++ build tools

This project uses native libraries to read RAR archives. On Windows you need the **MSVC linker** (`link.exe`). Install one of:

- [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) — select **Desktop development with C++**
- A full Visual Studio install with the same workload

If `cargo build` fails with `linker 'link.exe' not found`, this step is missing.

### Linux / macOS

A normal Rust toolchain is usually enough. On Linux you may need `build-essential` (or your distro’s equivalent C/C++ compiler package).

---

## First-time setup

### 1. Open the project

Clone or open this folder in Cursor (or any editor with a terminal). All commands below assume your terminal’s working directory is the project root — the folder that contains `Cargo.toml`.

In Cursor: **Terminal → New Terminal**. If the prompt is not already in the project folder:

```powershell
cd C:\path\to\daz-sailor
```

### 2. Configure your folders

Edit `directories.env` at the project root. Set the three paths for your machine:

| Variable | Purpose |
|----------|---------|
| `DAZ_SAILOR_DOWNLOADS` | Folder where you save new DAZ downloads (the app scans this) |
| `DAZ_SAILOR_DIM` | DAZ Install Manager **Downloads** queue folder |
| `DAZ_SAILOR_LIBRARY` | Your DAZ content library folder |

Example (Windows):

```env
DAZ_SAILOR_DOWNLOADS="C:/Users/You/Daz Downloads"
DAZ_SAILOR_DIM="C:/Users/Public/Documents/DAZ 3D/InstallManager/Downloads"
DAZ_SAILOR_LIBRARY="C:/Users/Public/Documents/My DAZ 3D Library"
```

Use forward slashes in Windows paths (they work the same as backslashes). Paths with spaces must be wrapped in double quotes.

Example (Linux with Wine):

```env
DAZ_SAILOR_DOWNLOADS="/home/you/Downloads/Daz Downloads"
DAZ_SAILOR_DIM="/home/you/.wine/drive_c/users/Public/Documents/DAZ 3D/InstallManager/Downloads"
DAZ_SAILOR_LIBRARY="/home/you/.wine/drive_c/users/Public/Documents/My DAZ 3D Library"
```

### 3. Generate `.env`

The app reads `.env` at startup. Sync it from `directories.env`:

**Windows (PowerShell):**

```powershell
.\scripts\sync-env.ps1
```

**Linux / macOS:**

```bash
chmod +x scripts/sync-env.sh
./scripts/sync-env.sh
```

You can edit `.env` directly for machine-specific overrides without changing `directories.env`. Re-run the sync script whenever you update `directories.env` and want those changes copied into `.env`.

> **Note:** `.env` is gitignored. `directories.env` is the shared template you commit; `.env` is local.

### 4. Build

```powershell
cargo build
```

For faster iteration during development, `cargo run` (below) builds automatically when needed.

---

## Running from a development environment

Use `cargo run` from the project root so the app can find `.env` or `directories.env`.

### Process everything in your downloads folder (default)

```powershell
cargo run
```

This is the same as `cargo run -- --mode entire-folder`. It scans `DAZ_SAILOR_DOWNLOADS`, processes each archive, and moves finished files to `<downloads>/done`.

### Dry run (no files written or moved)

```powershell
cargo run -- --dry-run
```

Use this first to see how archives would be classified without changing anything.

### Verbose output

```powershell
cargo run -- --verbose
```

### Process a single archive

```powershell
cargo run -- --file "C:\path\to\Some Product.rar"
```

Or pass the path as a positional argument:

```powershell
cargo run -- "C:\path\to\Some Product.rar"
```

### Demo mode

Runs three built-in sample file names from your configured downloads folder (useful for testing):

```powershell
cargo run -- --mode demo
```

### All CLI options

```powershell
cargo run -- --help
```

Common flags:

| Flag | Description |
|------|-------------|
| `--dry-run` | Show actions without writing or moving files |
| `--verbose` / `-v` | More detail during extraction |
| `--downloads-dir` | Override downloads folder |
| `--dim-downloads-dir` | Override DIM queue folder |
| `--daz-library-dir` | Override library folder |
| `--done-dir` | Override completed-archive folder (default: `<downloads>/done`) |

Environment variables and CLI flags use the same names; a CLI flag wins when both are set.

---

## Typical workflow in Cursor

1. Put new DAZ archives in your `DAZ_SAILOR_DOWNLOADS` folder.
2. Open the integrated terminal in Cursor (`Ctrl+`` ` or **View → Terminal**).
3. Run a dry run:

   ```powershell
   cargo run -- --dry-run
   ```

4. If the plan looks correct, run for real:

   ```powershell
   cargo run
   ```

5. For DIM packages, open DAZ Install Manager and install from its queue as usual. Library installs land directly in `DAZ_SAILOR_LIBRARY`.

---

## How classification works

1. The app opens the outer archive (`.zip`, `.rar`, or `.7z`).
2. If it contains inner archives, it inspects those too.
3. **DIM** — inner or outer content includes both `manifest.dsx` and `supplement.dsx` (case-insensitive). The DIM `.zip` is copied to the Install Manager downloads folder.
4. **Manual library** — paths contain known DAZ library top-level folders. Files are extracted into your library, with wrapper prefixes like `My Library/` stripped when detected.
5. If neither pattern matches, the archive is reported as failed and left in place.

---

## Troubleshooting

| Problem | What to try |
|---------|-------------|
| `DAZ_SAILOR_DOWNLOADS is not set` | Edit `directories.env`, run `.\scripts\sync-env.ps1`, or pass `--downloads-dir` |
| `downloads directory does not exist` | Create the folder or fix the path in `directories.env` |
| `link.exe` not found (Windows) | Install Visual Studio Build Tools with C++ workload |
| Archives not moved to `done` | Check for errors in the log; failed installs are not moved |
| Wrong library/DIM path | Update `directories.env` and re-sync, or use CLI overrides |

---

## Release build (optional)

To build a standalone executable without running through Cargo each time:

```powershell
cargo build --release
```

The binary will be at `target\release\daz-sailor.exe` (Windows) or `target/release/daz-sailor` (Linux/macOS). Run it from the project root (or any directory where `.env` is present) the same way as `cargo run`.
