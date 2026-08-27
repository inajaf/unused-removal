# PROJECT PLAN — Unused Removal Utility (Windows)

## 1. Цель проекта
Создать быструю, безопасную утилиту для поиска и удаления больших / ненужных файлов на Windows с интуитивным интерфейсом. Инструмент должен:
- Быстро сканировать диск (параллельно, без перегрузки).
- Классифицировать файлы по явным эвристике (размер, возраст, каталоги, расширения).
- Предлагать удаление в безопасном режиме (квартин, не hard-delete).

---

## 2. Язык программирования: Go
**Почему Go — «самый быстрый на практике» для этой задачи:**

| Критерий | Почему Go |
|----------|-----------|
| **Скорость** | Compiled binary + GC → запуск ~0,1 с; параллелизм `goroutine` + `sync.WaitGroup`. |
| **Память** | Минимальный overhead на структуру данных (slice, map); no reflection needed. |
| **Конкурентность** | `go run --race` + `sync/atomic`; нет data-race в многопоточном поиске. |
| **Сборка / распространение** | `go build → single binary`, cross-platform, zero runtime deps при `-ldflags "-w -s"`. |
| **Файловая система** | `filepath.WalkDir` (Go 1.16+) + `os.Stat()`; no FFI, нет Win32 API boilerplate. |
| **Экосистема** | `chi` / `gin` для HTTP-сервера; `template` — для встраивания HTML; `logrus`/`slog`; `golang.org/x/sys/unix` только при необходимости. |

> **Альтернативы и почему они отклонены:**
> - Rust: быстрее, но больший boilerplate + longer compile times; нет готовых HTTP-фреймворков в stdlib.
> - Python: медленнее (GIL), не подходит для heavy I/O.
> - C/C++: ошибка-устойчивость на уровне ОС требует ручного управления памятью, сложно сделать safe UI.

---

## 3. Архитектура

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐     ┌──────────────┐
│   CLI / Web   │────▶│   Scanner   │────▶│  Classifier  │────▶│  Quarantine  │
│  (UI)         │     │ (parallel)  │     │  (rules)     │     │  (move/trim) │
└──────────────┘     └─────────────┘     └──────────────┘     └──────────────┘
```

### 3.1 Сканирование (`scanner`)
- `filepath.WalkDir(root, fn)` с `os.Stat()` → size, mtime, size.
- Parallelism: `sync.Pool` + worker pool (configurable via `-workers`).
- Exclude dirs / prefixes из config.
- Результат: `[]ScanResult {Path string, Size uint64, MTime time.Time}`.

### 3.2 Классификация (`classifier`)
Правила (в порядке приоритета):
1. **Junk** — расширение в `junk_extensions` или путь под `junk_dirs`.
2. **Huge** — size > `huge_bytes`.
3. **Large** — size > `large_bytes`.
4. **Stale** — mtime < cutoff (configurable days).
5. **OldLog** — mtime < logCutoff AND (path contains `/logs` или extension `.log`, `.txt`, etc.).
6. **StaleInstall** — mtime < installCutoff в `/ProgramData\Microsoft\Windows\...`.

### 3.3 Безопасное удаление (`quarantine`)
- **Никогда не hard-delete.** Все файлы / папки move в `Quarantine` каталог (по умолчанию `%LOCALAPPDATA%\unused-removal\Quarantine`).
- Пользователь:
  - Просматривает список в UI.
  - Двойный клик → "Delete permanently" (only after confirming in dialog).
  - Single click → "Move to Quarantine".
- Quarantine — реальный каталог; файлы там доступны для восстановления.

### 3.4 Интерфейс
- **Desktop** по умолчанию: HTML + JS внутри нативного WebView, без HTTP-сервера и портов.
- **CLI** (`./usrm -s C:\`): fast one-shot scan + table output.
- UI: Bootstrap-like CSS inline, no external deps.

---

## 4. Конфигурация (config.toml)

```toml
# Общие
root = "C:\"
workers = 0          # auto = number of CPUs
follow_links = false

exclude_dirs = [
    "$Recycle.Bin",
    "System Volume Information",
    "Windows\\WinSxS",
    "Windows\\SoftwareDistribution",
    "ProgramData\\Microsoft\\Windows Defender"
]

# Правила поиска
large_bytes = 100 * 1024 * 1024    # 100 МБ
huge_bytes = 500 * 1024 * 1024     # 500 МБ
stale_days = 180                   # days without change → "stale"
old_log_days = 30                  # days without change → old logs (junk)
stale_install_days = 90            # days without change → stale installers

junk_extensions = ["~$*", ".tmp", ".temp", ".bak", ".old", ".dmp", ".chk"]
junk_dirs = [
    "%TEMP%",
    "C:\\Windows\\Temp",
    "C:\\Windows\\Prefetch"
]

check_duplicates = false            # future feature

# Безопасность
protect_system = true                # deny scanning protected paths by default
allow_protected = false             # user must explicitly allow

use_cache = true                    # cache stat results
cache_dir = ""                      # empty = auto (per-run temp dir)


[Quarantine]
path = "%LOCALAPPDATA\\unused-removal\\Quarantine"
```

---

## 5. UI Design (Web)

### 5.1 Структура страницы
```
┌─────────────────────┐
│  Unused Removal     │  ← Header
│  ─────────────────  │
│  [root: C:\]       │  ← Config bar
│  [Workers: auto]   │
│                     │
│  ▸ Scan (Ctrl+S)    │  ← Main action
├─────────────────────┤
│  Results Table      │  ← List of files + metrics
│  ┌─────────┬────────┬────────┬──────────┐
│  │ Path    │ Size   │ Age    │ Status   │
│  └─────────┴────────┴────────┴──────────┘
│                     │
│  [Move to Quarantine]  [Delete Permanently]
├─────────────────────┤
│  Summary: X files, Y GB total          │
└─────────────────────┘
```

### 5.2 Flow
1. Пользователь вводит root (или использует C:\).
2. Нажимает **Scan** → сканирование в фоне.
3. После завершения — таблица результатов.
4. Каждый файл:
   - **Single click**: Move to Quarantine.
   - **Double-click**: Delete Permanently (confirm dialog).
5. `Clear` button clears the table and quarantine status for next scan.

### 5.3 Styling
- Inline CSS (Bootstrap-like, ~10KB).
- No external CDNs.
- Responsive design (mobile/tablet friendly).

---

## 6. Пошаговый план реализации

### Фаза 1 — Core Scan Engine (Week 1–2)
- [ ] `scanner.go`: parallel walk with `filepath.WalkDir`.
- [ ] `classifier.go`: rules engine, `ScanResult` → `Category {Junk|Large|Huge|Stale|OldLog|StaleInstall}`.
- [ ] `config.go`: TOML parse, defaults, env var expansion.
- [ ] **CLI**: `./usrm -s C:\ --workers 8` → table output + summary.

### Фаза 2 — Quarantine & Safety (Week 3)
- [ ] `quarantine.go`: move-to-quarantine logic.
- [ ] Config: `protect_system`, `allow_protected`.
- [ ] Hard-delete ONLY after explicit double-click confirm.
- [ ] Quarantine log (`quarantine.log`) — JSON, append-only.

### Фаза 3 — Desktop UI (Week 4–5)
- [ ] Internal desktop request router (no network listener).
- [ ] HTML template inline: table, buttons, summary.
- [ ] Client-side JS: scan trigger, move/delete actions.
- [ ] JSON API endpoints: `/scan`, `/results`, `/quarantine`.

### Фаза 4 — Polish & Testing (Week 6)
- [ ] `go test -race` + benchmarks (`go test -bench`).
- [ ] Load test: scan of large tree (~10M files), measure time.
- [ ] Edge cases: symlinks, permission denied, long paths (>255 chars).
- [ ] README.md с примерами запуска и настройками.

### Фаза 5 — Launch (Week 7)
- [ ] Build: `go build -o usrm ./cmd`.
- [ ] Run CLI: `./usrm -s C:\` → verify output.
- [ ] Run Web: `./usrm --web` → open browser, scan C:\.

---

## 7. Сложные моменты и решения

| Проблема | Решение |
|----------|---------|
| **Slow scan on huge trees** | Parallel workers + `filepath.WalkDir`; cache stat results; exclude system dirs upfront. |
| **Permission denied** | Log warning, skip file; option `--force` to retry with elevated privileges (optional). |
| **Protected paths** | Deny by default (`protect_system=true`); user must enable `allow_protected`. |
| **Hard-delete safety** | Quarantine only; permanent delete requires double-click + confirmation. |
| **Large files / long paths** | Handle 256–4096 char names; use `os.Stat()` directly (no path length limit in Go). |

---

## 8. Ожидаемая производительность (оценка)

| Сценарий | Оценка времени |
|----------|---------------|
| Scan C:\ (10M files, 50 GB) | ~30–60 сек при 4–8 workers |
| Scan C:\Users\ (2M files, 20 GB) | ~10–20 sec |
| Classify + Quarantine | 2× scan time (sequential post-scan) |

---

## 9. Безопасность — чек-лист

- [ ] Never hard-delete from system paths without explicit user opt-in.
- [ ] Quarantine is a real folder; files can be recovered manually.
- [ ] `protect_system=true` по умолчанию (system directories excluded).
- [ ] Double-click confirm before permanent delete.
- [ ] No background processes started; no network access.
- [ ] All temp files cleaned up after run.

---

## 10. Следующие шаги (имmediately)

1. **Выберите интерфейс**: TUI (CLI-only, table in terminal) или нативный Desktop UI?  
   *(Это влияет на UI-компоненты и архитектуру API; ответните, чтобы я адаптировал план под ваш выбор.)*

2. **Уточните приоритеты**:  
   - First: speed (fastest possible scan) vs completeness (scan every corner).  
   - First: safety (maximum quarantine checks) vs convenience (fewer confirmations).

3. **Приоритетные эвристики** для первой версии:
   - Junk extensions (`~$*`, `.tmp`, `.bak`) — always junk.
   - Junk dirs (`%TEMP%`, `%LOCALAPPDATA%\...`) — always excluded.
   - Files > 100 MB — flagged as large.
   - Files > 20 days old (no change) — stale, move to quarantine.

---

> **Резюме**:  
> Go + parallel filesystem walk + explicit classification rules + quarantine-first deletion policy = быстрый, безопасный, интуитивный инструмент для Windows. Desktop UI — основной интерфейс; CLI — быстрый fallback. План по 7 неделям при одном разработчике.
