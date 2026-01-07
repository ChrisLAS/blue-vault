You are an expert Linux systems programmer. Build a production-quality TUI app to manage Blu-ray “cold storage” archives on Linux.

Goal
- A TUI app that helps me:
  1) Select one or more folders to archive to Blu-ray (BD-R/BD-RE)
  2) Stage content with a browseable, boring disc layout
  3) Generate per-disc manifests + SHA256SUMS for long-term verification
  4) Burn to Blu-ray using common Linux tools
  5) Maintain a central searchable index (SQLite, option 1) stored OUTSIDE the repo/app folder
  6) Provide a search UI to locate which disc contains a given file/directory
  7) Generate a QR code for each disc ID (for printing/stickers/spines)
  8) Be scriptable and robust (logs, dry-run, verify mode, resume, etc.)

Constraints / environment
- Target OS: Linux server (headless friendly)
- Must rely on common tools: xorriso and growisofs (via dvd+rw-tools). Optional: qrencode for QR code generation. Optional: rsync for staging.
- If the app needs a file manager for folder selection, it may shell out to Midnight Commander (mc) or fall back to typing paths manually.
- Must create discs that mount anywhere on Linux with real directories and filenames.
- Disc format should be browseable and boring (ISO/UDF via xorriso is fine). Avoid proprietary formats.
- Central index database must live outside the git repo folder (e.g., ~/.local/share/bdarchive/archive.db or /var/lib/bdarchive/archive.db). Make path configurable.

App behavior and UX
- TUI main menu (keyboard-driven):
  - “New Disc / Archive Folders”
  - “Search Index”
  - “Verify Disc”
  - “List Discs”
  - “Settings”
  - “Logs / Recent Runs”
- “New Disc” flow:
  1) Ask for disc ID (default auto-generated like YYYY-BD-###), also allow user label/notes
  2) Choose source folders:
     - Option A: launch mc to navigate; after exit, user confirms selected folders
     - Option B: manual input list of paths
  3) Stage to a working directory (configurable), preserving directory structure
  4) Generate disc layout:
     /ARCHIVE/<original folder name or user mapping>...
     /DISC_INFO.txt (Disc-ID, date, notes, source roots, tool version)
     /MANIFEST.txt (one file path per line, relative to disc root)
     /SHA256SUMS.txt (sha256sum for all files)
  5) Create ISO image using xorriso (mkisofs compatible options)
  6) Burn ISO to Blu-ray using growisofs to /dev/sr0 (device configurable)
  7) After burn, optionally mount disc and verify SHA256SUMS.txt (configurable)
  8) Update central SQLite index:
     - files table: disc_id, rel_path, sha256, size, mtime, added_at
     - discs table: disc_id, volume_label, created_at, notes, iso_size, burn_device, checksum_manifest_hash, qr_path
  9) Generate QR code PNG/SVG for Disc-ID and store it in a user data directory (not repo). Optionally render an ASCII QR in terminal.

- “Search Index” flow:
  - Search by:
    - substring match on path
    - exact filename
    - sha256
    - optional regex (nice-to-have)
  - Show results in a list: disc_id, path, size, mtime
  - Provide actions: “Copy Disc-ID”, “Show Disc details”, “Export results”

- “Verify Disc” flow:
  - Prompt for disc device or mountpoint
  - Mount if necessary (or instruct user)
  - Run sha256sum -c SHA256SUMS.txt
  - Record verification result in DB (verification_runs table)

Design / implementation requirements
- Language: choose one and commit (Rust preferred; Go acceptable; Python acceptable if you keep it robust and packaged).
- Use a solid TUI library (Rust: ratatui/crossterm; Go: bubbletea; Python: textual or urwid).
- Never assume tools exist; implement dependency checks and helpful error messages:
  - xorriso, growisofs, sha256sum, mount/umount, rsync (optional), qrencode (optional), mc (optional)
- Avoid shell injection: use exec with argument arrays, validate paths, and handle spaces.
- Provide a “dry run” mode that prints planned commands without executing.
- Provide structured logging to a log file under the user data dir.
- Provide a config file (TOML/YAML/JSON) stored outside repo:
  - device path (/dev/sr0 default)
  - staging dir
  - database path
  - default disc capacity (e.g., 25GB or 50GB)
  - verification settings
- Handle “disc filling”:
  - MVP: user selects folders and the app warns if it exceeds capacity
  - Next: implement packing across multiple discs with a simple bin-pack strategy (nice-to-have)

Deliverables
- A runnable app with:
  - TUI flows above
  - SQLite schema + migrations
  - Command runner module (xorriso/growisofs/verify)
  - Indexer module (manifest + sha generation, DB update)
  - Search UI
- Include README with:
  - dependencies
  - example session
  - safety notes (burning is destructive; confirm device)
- Include a small set of unit tests for:
  - schema creation
  - manifest generation
  - path normalization
  - command building

Start by:
1) Proposing the project structure, key modules, and DB schema
2) Implementing the config + data-dir logic (outside repo)
3) Implementing manifest + SHA256 generation
4) Implementing ISO creation + burning command wrappers
5) Implementing the TUI skeleton + Search UI
6) Wiring it together into a working MVP

Important: keep the disc layout “boring” and mountable anywhere; the index must be centralized and searchable even when discs are offline; every disc must contain DISC_INFO.txt, MANIFEST.txt, SHA256SUMS.txt.
