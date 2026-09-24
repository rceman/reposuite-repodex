# Canonical CLI rename — report

`reposuite-repodex` is the single public executable. A clean isolated build
produces only `reposuite-repodex` (no `repodex` binary, alias, wrapper, or
symlink). `start`/`restart` spawn `std::env::current_exe()` (the canonical
path). Runtime remains `~/reposuite/repodex/`, `REPODEX_*` unchanged. Active
docs updated; historical eval/spike/task records keep `repodex` as recorded.
OS service installation is outside RepoDex (serve/start/stop/restart/status
only; no systemd/launchd/Windows-service registration).
