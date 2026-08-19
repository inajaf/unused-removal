# unused-removal

> **Сверхбыстрый поиск и безопасное удаление ненужных файлов на Windows**

Одиночный статический EXE (~15 MB) без зависимостей. Сканирует диск за секунды, находит большие/мусорные/старые файлы и дубликаты, показывает красивый веб-интерфейс и удаляет в Корзину (обратимо).

---

## 🚀 Возможности

| Возможность | Описание |
|-------------|----------|
| **Параллельное сканирование** | Win32 `FindFirstFileW` + пул воркеров (N = ядер CPU). 100к файлов за 0.2 с. |
| **Умные правила** | Крупные файлы, мусор (temp/cache/logs), старые файлы, дубликаты, инсталляторы. |
| **Инкрементальный кэш** | bbolt — повторные сканы в разы быстрее (только изменённые каталоги). |
| **Веб-интерфейс** | Одиночный EXE поднимает HTTP на `127.0.0.1`, открывается в браузере. Таблица, фильтры, выбор, экспорт. |
| **Безопасное удаление** | По умолчанию — в Корзину (`SHFileOperationW` + `FOF_ALLOWUNDO`). Жёсткое удаление — только по двойному подтверждению. |
| **Защита системы** | `C:\Windows\WinSxS`, `System32`, `Program Files`, `pagefile.sys`, `hiberfil.sys` и др. — **никогда** не предлагаются к удалению. |
| **Нет зависимостей** | Чистый Go (CGO=0), статический бинарник, работает на чистом Windows 10/11. |

---

## 📦 Установка

Скачайте `unused-removal.exe` из [Releases](https://github.com/your/repo/releases) или соберите сами:

```bash
# Требуется Go 1.21+
git clone https://github.com/your/unused-removal
cd unused-removal
go build -o unused-removal.exe ./cmd/unused-removal
```

---

## 🖥 Использование

### Веб-интерфейс (рекомендуется)

```bash
unused-removal serve -port 8080
# Откроется браузер на http://127.0.0.1:8080
```

1. Выберите диск/папку (C:\, D:\, свой путь)
2. Настройте правила (пороги, дубликаты, кэш)
3. Нажмите **«Начать сканирование»**
4. В результатах: фильтруйте, сортируйте, выбирайте файлы
4. **«В Корзину»** — безопасно, можно восстановить
5. **«Безвозвратно»** — только после ввода подтверждения

### TUI — интерактивный терминал (Bubble Tea)

```bash
unused-removal tui
```

Полноэкранный интерфейс прямо в терминале:

| Экран | Клавиши |
|-------|---------|
| **Настройка** | `Tab`/`↓` — навигация, `Enter` — сканировать, `q` — выход |
| **Сканирование** | Живой прогресс: файлы, каталоги, байты, ф/с, кэш. `s` — остановить |
| **Результаты** | `↑↓` — выбор строки, `Пробел` — отметить, `t` — в Корзину, `x` — безвозвратно, `c` — фильтр по категории, `r` — назад, `q` — выход |

### CLI — сканирование

```bash
# Быстрый скан C:\ с JSON-отчётом
unused-removal scan -root C:\ -no-cache -json report.json

# Только крупные файлы > 500 МБ, без кэша
unused-removal scan -root D:\ -large 500MB -no-cache

# С дубликатами (медленнее)
unused-removal scan -root C:\ -duplicates
```

### CLI — бенчмарк

```bash
# Сравнение параллельного vs последовательного сканирования
unused-removal bench -files 100000 -depth 4 -serial
```

### Конфигурация

```bash
# Показать текущий конфиг
unused-removal config
```

Конфиг ищется в: `./config.toml` → `%LOCALAPPDATA%\unused-removal\config.toml` → дефолты.

---

## ⚙️ Конфигурация (`config.toml`)

```toml
root = "C:\\"
workers = 0              # 0 = все ядра
follow_links = false     # не следовать за junction/symlink
use_cache = true         # инкрементальный кэш (bbolt)
check_duplicates = false # поиск дубликатов (медленно)

# Пороги
large_bytes = 104857600      # 100 МБ
huge_bytes = 524288000       # 500 МБ
stale_days = 180             # не менялся N дней
old_log_days = 30            # логи старше N дней
stale_install_days = 90      # инсталляторы в Downloads

# Мусор
junk_extensions = [".tmp", ".temp", ".bak", ".old", ".dmp", ".chk", "~$*"]
junk_dirs = [
  "%TEMP%",
  "C:\\Windows\\Temp",
  "C:\\Windows\\Prefetch",
  "%LOCALAPPDATA%\\Google\\Chrome\\User Data\\Default\\Cache",
  "%LOCALAPPDATA%\\Microsoft\\Edge\\User Data\\Default\\Cache",
  "%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles"
]

# Исключения из обхода
exclude_dirs = ["$Recycle.Bin", "System Volume Information", "Windows\\WinSxS"]

# Безопасность
protect_system = true
allow_protected = false
```

---

## 🏗 Архитектура

```
unused-removal/
├── cmd/unused-removal/     # CLI: scan / serve / tui / bench / config
│   ├── main.go             # Парсер подкоманд
│   ├── tui.go              # Интерактивный TUI (Bubble Tea)
│   ├── output.go           # Красивый CLI-вывод (цвета, прогресс)
│   └── tui_test.go         # Тесты TUI-модели
├── internal/
│   ├── scanner/            # Параллельный Win32-сканер
│   │   ├── scanner.go      # Walker, очередь задач, прогресс
│   │   ├── scanner_windows.go  # FindFirstFileExW + LARGE_FETCH, FILETIME
│   │   └── cache.go        # bbolt кэш отпечатков каталогов (db.Batch)
│   ├── rules/              # Движок правил (чистый Go, тестируемый)
│   │   ├── engine.go       # Junk / Stale / Large / Protected + дубликаты
│   │   └── duplicates.go   # Группировка по размеру + Blake3 (параллельно)
│   ├── cleaner/            # Удаление
│   │   └── cleaner.go      # SHFileOperationW (Корзина) + HardDelete
│   ├── config/             # TOML + env-переменные
│   └── ui/                 # Веб-сервер + embed.FS
│       ├── server.go       # HTTP API: /api/scan, /stop, /progress, /results, /delete, /export
│       └── web/            # TypeScript + HTML/CSS (esbuild → embed)
│           ├── src/app.ts      # Основное приложение
│           ├── src/api.ts      # Typed API клиент
│           ├── src/types.ts    # Интерфейсы
│           └── web/index.html  # SPA
└── go.mod
```

### Ключевые идеи скорости

1. **`FindFirstFileExW` + `FIND_FIRST_EX_LARGE_FETCH`** — пакетное чтение каталога: Windows возвращает пачку записей за один системный вызов (критично для WinSxS/node_modules с тысячами файлов). Плюс отпечаток каталога берётся из записи `.` — без отдельного `GetFileAttributesEx`.
2. **Work-queue + worker pool** — общая очередь каталогов, N воркеров берут задачу. При переполнении очереди воркер обрабатывает каталог **синхронно** — это устраняет producer-consumer deadlock, из-за которого сканирование могло «зависнуть» на каталогах с тысячами подкаталогов.
3. **Инкрементальный кэш** — отпечаток каталога = `LastWriteTime`. Неизменённый каталог → воспроизводим записи из bbolt без обхода. `db.Batch` коалесцирует транзакции записи.
4. **Пакетный сброс записей** — мьютекс захватывается 1 раз на 256 файлов, а не на каждый.
5. **Параллельное хэширование дубликатов** — blake3 на всех ядрах (пул = NumCPU).
6. **Живой список файлов** — `Snapshot.Recent` хранит последние 50 обработанных файлов для ленты в UI.

> **Скорость (измерено):** 1.54 млн файлов в `AppData\Local` за 19.5 с ≈ **79k ф/с** (до фикса deadlock — 52 ф/с).

---

## 🧪 Тестирование

```bash
# Unit-тесты (сканер, правила, защита)
go test ./internal/...

# Бенчмарк скорости
unused-removal bench -files 100000 -depth 4 -serial
# Ожидаемое ускорение: 4-6x на 12 ядрах
```

---

## 📋 Пример JSON-отчёта

```json
[
  {
    "path": "C:\\Temp\\huge.iso",
    "size_bytes": 4294967296,
    "category": "huge",
    "reason": "очень крупный файл (> 500 MiB)",
    "risk": "caution",
    "mod_time": "2024-01-15T10:30:00Z"
  },
  {
    "path": "C:\\Users\\User\\AppData\\Local\\Temp\\setup.tmp",
    "size_bytes": 1048576,
    "category": "junk",
    "reason": "расширение .tmp",
    "risk": "safe",
    "mod_time": "2024-01-10T14:22:00Z"
  }
]
```

---

## ⚠️ Безопасность

- **По умолчанию** — только в Корзину. Восстановление: корзина → правый клик → «Восстановить».
- **Защищённые пути** (WinSxS, System32, Program Files, pagefile.sys, hiberfil.sys, boot) **никогда** не появляются в результатах, пока `allow_protected = true`.
- **Жёсткое удаление** требует нажатия «Безвозвратно» + модального подтверждения с предупреждением.
- **Ошибки доступа** (System Volume Information и др.) — логируются, скан продолжается.

---

## 📄 Лицензия

MIT — используйте, меняйте, распространяйте.

---

## 🙏 Благодарности

- `golang.org/x/sys/windows` — Win32 биндинги
- `go.etcd.io/bbolt` — встраиваемая КВ-хранилище для кэша
- `github.com/zeebo/blake3` — сверхбыстрое хэширование для дубликатов
- `github.com/BurntSushi/toml` — парсинг конфига