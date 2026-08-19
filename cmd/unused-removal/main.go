package main

import (
	"context"
	"encoding/csv"
	"encoding/json"
	"flag"
	"fmt"
	"log"
	"os"
	"path/filepath"
	"runtime"
	"runtime/pprof"
	"sort"
	"strings"
	"time"

	"unused-removal/internal/config"
	"unused-removal/internal/rules"
	"unused-removal/internal/scanner"
	"unused-removal/internal/ui"
)

var (
	version = "dev"
	commit  = "none"
	date    = "unknown"
)

func main() {
	// Глобальные флаги
	configPath := flag.String("config", "", "путь к config.toml (по умолчанию автопоиск)")
	cpuprofile := flag.String("cpuprofile", "", "записать CPU-профиль в файл")
	memprofile := flag.String("memprofile", "", "записать профиль памяти в файл")
	showVersion := flag.Bool("version", false, "показать версию и выйти")
	flag.Parse()

	if *showVersion {
		fmt.Printf("unused-removal %s (%s) built %s\n", version, commit, date)
		return
	}

	if *cpuprofile != "" {
		f, err := os.Create(*cpuprofile)
		if err != nil {
			log.Fatal(err)
		}
		defer f.Close()
		if err := pprof.StartCPUProfile(f); err != nil {
			log.Fatal(err)
		}
		defer pprof.StopCPUProfile()
	}

	args := flag.Args()
	if len(args) == 0 {
		// Нет подкоманды → интерактивный TUI (Bubble Tea), если есть терминал,
		// иначе веб-интерфейс (для запуска из ярлыка/CI без TTY).
		if isTerminal() {
			tuiCmd(nil)
		} else {
			serveCmd(nil)
		}
		return
	}

	switch args[0] {
	case "scan":
		scanCmd(args[1:], *configPath)
	case "serve":
		serveCmd(args[1:])
	case "tui":
		tuiCmd(args[1:])
	case "bench":
		benchCmd(args[1:], *configPath)
	case "config":
		configCmd(args[1:])
	default:
		fmt.Fprintf(os.Stderr, "неизвестная команда: %s\n", args[0])
		printUsage()
		os.Exit(1)
	}

	if *memprofile != "" {
		f, err := os.Create(*memprofile)
		if err != nil {
			log.Fatal(err)
		}
		defer f.Close()
		runtime.GC()
		pprof.WriteHeapProfile(f)
	}
}

func printUsage() {
	fmt.Println(`unused-removal — быстрый поиск и удаление ненужных файлов на Windows

Команды:
  (без аргументов)         интерактивный TUI (Bubble Tea)
  scan [flags] [path]      сканировать путь (по умолчанию из конфига)
  serve [flags]            запустить веб-интерфейс
  tui                      интерактивный терминальный интерфейс (Bubble Tea)
  bench [flags]            бенчмарк скорости на синтетической фикстуре
  config                   показать текущий конфиг

Глобальные флаги:
  -config string   путь к config.toml
  -version         показать версию
  -cpuprofile      записать CPU-профиль
  -memprofile      записать профиль памяти`)
}

// scanCmd — CLI-сканирование.
func scanCmd(args []string, configPath string) {
	fs := flag.NewFlagSet("scan", flag.ExitOnError)
	root := fs.String("root", "", "корневой путь для сканирования (переопределяет конфиг)")
	jsonOut := fs.String("json", "", "вывести отчёт в JSON файл")
	csvOut := fs.String("csv", "", "вывести отчёт в CSV файл")
	noCache := fs.Bool("no-cache", false, "отключить инкрементальный кэш")
	workers := fs.Int("workers", 0, "число воркеров (0 = NumCPU)")
	follow := fs.Bool("follow-links", false, "следовать за junction/symlink")
	duplicates := fs.Bool("duplicates", false, "искать дубликаты (медленно)")
	protect := fs.Bool("protect", true, "защищать системные пути")
	top := fs.Int("top", 10, "показать топ N находок по размеру (0 = не показывать)")
	fs.Parse(args)

	// Загружаем конфиг
	cfg, err := config.Load(configPath)
	if err != nil {
		log.Fatalf("config load: %v", err)
	}

	// Переопределения флагами
	if *root != "" {
		cfg.Root = *root
	}
	if *workers > 0 {
		cfg.Workers = *workers
	}
	if *follow {
		cfg.FollowLinks = true
	}
	if *duplicates {
		cfg.CheckDuplicates = true
	}
	cfg.ProtectSystem = *protect
	if *noCache {
		cfg.UseCache = false
	}

	fmt.Printf("Сканирование: %s (воркеров: %d)...\n", cfg.Root, cfg.Workers)
	start := time.Now()

	// Сканер
	prog := scanner.NewProgress()
	opts := *cfg.ScannerOptions()
	w := scanner.New(opts, prog, nil)
	// Кэш (опционально)
	var cache scanner.Cache
	if cfg.UseCache {
		hash := configHash(cfg)
		c, err := scanner.NewBoltCache("unused-removal", hash)
		if err != nil {
			log.Printf("warning: cache disabled: %v", err)
		} else {
			cache = c
			defer c.Close()
			w = scanner.New(opts, prog, cache)
		}
	}

	// Живой прогресс в stderr (не ломает перенаправление stdout в файл)
	done := make(chan struct{})
	go printLiveProgress(prog, done)

	ctx := context.Background()
	recs, errs, err := w.Walk(ctx, cfg.Root)
	close(done) // останавливаем прогресс-бар
	if err != nil {
		log.Fatalf("scan failed: %v", err)
	}

	// Правила
	engine := rules.NewEngine(cfg)
	findings := engine.Analyze(recs)
	findings = engine.FilterProtected(findings)

	// Дубликаты (опционально)
	if cfg.CheckDuplicates {
		dups := engine.FindDuplicates(recs)
		findings = append(findings, dups...)
	}

	// Сортировка: по убыванию размера
	sort.Slice(findings, func(i, j int) bool {
		return findings[i].Size > findings[j].Size
	})

	elapsed := time.Since(start)
	fmt.Printf("\nГотово за %.2fs. Найдено файлов: %d, находок: %d, ошибок: %d\n",
		elapsed.Seconds(), len(recs), len(findings), len(errs))

	// Красивая сводка по категориям и топ находок
	printHeader("Результаты")
	printSummaryTable(findings)
	if *top > 0 {
		printTopFindings(findings, *top)
	}

	// Вывод в файлы
	if *jsonOut != "" {
		if err := writeJSON(*jsonOut, findings); err != nil {
			log.Printf("json write: %v", err)
		}
	}
	if *csvOut != "" {
		if err := writeCSV(*csvOut, findings); err != nil {
			log.Printf("csv write: %v", err)
		}
	}
}

// writeJSON пишет находки в JSON.
func writeJSON(path string, findings []rules.Finding) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	enc := json.NewEncoder(f)
	enc.SetIndent("", "  ")
	return enc.Encode(findings)
}

// writeCSV пишет находки в CSV.
func writeCSV(path string, findings []rules.Finding) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	w := csv.NewWriter(f)
	defer w.Flush()
	// Header
	if err := w.Write([]string{"path", "size_bytes", "category", "reason", "risk", "mod_time"}); err != nil {
		return err
	}
	for _, f := range findings {
		rec := []string{
			f.Path,
			fmt.Sprintf("%d", f.Size),
			string(f.Category),
			f.Reason,
			string(f.Risk),
			f.ModTime.Format(time.RFC3339),
		}
		if err := w.Write(rec); err != nil {
			return err
		}
	}
	return nil
}

func formatBytes(b int64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%d B", b)
	}
	div, exp := int64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %ciB", float64(b)/float64(div), "KMGTPE"[exp])
}

// configHash — тот же, что в scanner (простой FNV для инвалидации кэша).
func configHash(cfg *config.Config) string {
	var sb strings.Builder
	sb.WriteString("w:")
	sb.WriteString(itoa(cfg.Workers))
	sb.WriteString(" fl:")
	sb.WriteString(itoa(boolToInt(cfg.FollowLinks)))
	for _, e := range cfg.ExcludeDirs {
		sb.WriteString(" x:")
		sb.WriteString(e)
	}
	for _, p := range cfg.ExcludePrefix {
		sb.WriteString(" xp:")
		sb.WriteString(p)
	}
	var h uint64 = 1469598103934665603
	for i := 0; i < sb.Len(); i++ {
		h ^= uint64(sb.String()[i])
		h *= 1099511628211
	}
	return fmt.Sprintf("%x", h)
}

func itoa(i int) string {
	if i == 0 {
		return "0"
	}
	var b [32]byte
	n := len(b)
	neg := i < 0
	if neg {
		i = -i
	}
	for i > 0 {
		n--
		b[n] = byte('0' + i%10)
		i /= 10
	}
	if neg {
		n--
		b[n] = '-'
	}
	return string(b[n:])
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}

// serveCmd — запуск веб-интерфейса.
func serveCmd(args []string) {
	fs := flag.NewFlagSet("serve", flag.ExitOnError)
	port := fs.Int("port", 0, "порт (0 = авто)")
	fs.Parse(args)

	cfg, err := config.Load("")
	if err != nil {
		log.Fatalf("config load: %v", err)
	}
	if *port > 0 {
		cfg.WebPort = *port
	}

	server := ui.NewServer(cfg)
	addr := fmt.Sprintf("127.0.0.1:%d", cfg.WebPort)
	fmt.Printf("Веб-интерфейс: http://%s\n", addr)
	if err := server.Start(addr); err != nil {
		log.Fatalf("server: %v", err)
	}
}

// benchCmd — бенчмарк на синтетической фикстуре.
func benchCmd(args []string, configPath string) {
	fs := flag.NewFlagSet("bench", flag.ExitOnError)
	files := fs.Int("files", 100000, "число файлов в фикстуре")
	depth := fs.Int("depth", 4, "глубина дерева")
	serial := fs.Bool("serial", false, "запустить последовательный вариант для сравнения")
	fs.Parse(args)

	fmt.Printf("Бенчмарк: %d файлов, глубина %d\n", *files, *depth)

	// Создаём временную фикстуру
	tmpDir, err := os.MkdirTemp("", "unused-bench-*")
	if err != nil {
		log.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	fmt.Println("Создание фикстуры...")
	start := time.Now()
	if err := createFixture(tmpDir, *files, *depth); err != nil {
		log.Fatalf("fixture: %v", err)
	}
	fmt.Printf("Фикстура готова за %.2fs\n", time.Since(start).Seconds())

	// Параллельный скан
	cfg, _ := config.Load(configPath)
	cfg.Root = tmpDir
	cfg.Workers = runtime.NumCPU()
	cfg.UseCache = false
	opts := *cfg.ScannerOptions()

	prog := scanner.NewProgress()
	w := scanner.New(opts, prog, nil)

	ctx := context.Background()
	t0 := time.Now()
	recs, _, err := w.Walk(ctx, tmpDir)
	parTime := time.Since(t0)
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Параллельный: %.2fs, %d файлов, %.0f ф/с\n", parTime.Seconds(), len(recs), float64(len(recs))/parTime.Seconds())

	// Последовательный скан (опционально)
	if *serial {
		cfg.Workers = 1
		opts2 := *cfg.ScannerOptions()
		prog2 := scanner.NewProgress()
		w2 := scanner.New(opts2, prog2, nil)
		t1 := time.Now()
		recs2, _, _ := w2.Walk(ctx, tmpDir)
		serTime := time.Since(t1)
		fmt.Printf("Последовательный: %.2fs, %d файлов, %.0f ф/с\n", serTime.Seconds(), len(recs2), float64(len(recs2))/serTime.Seconds())
		if serTime > parTime {
			fmt.Printf("Ускорение: %.2fx\n", float64(serTime)/float64(parTime))
		}
	}
}

// createFixture создаёт дерево тестовых файлов.
func createFixture(root string, totalFiles, depth int) error {
	// Простая генерация: распределяем файлы по каталогам
	filesPerDir := totalFiles / 100
	if filesPerDir < 1 {
		filesPerDir = 1
	}
	dirs := totalFiles / filesPerDir
	if dirs < 1 {
		dirs = 1
	}

	var createDir func(path string, d int) error
	createDir = func(path string, d int) error {
		if d >= depth {
			return nil
		}
		for i := 0; i < 10; i++ { // 10 подкаталогов на уровень
			sub := filepath.Join(path, fmt.Sprintf("dir_%d_%d", d, i))
			if err := os.MkdirAll(sub, 0o755); err != nil {
				return err
			}
			// Файлы в этом каталоге
			for j := 0; j < filesPerDir; j++ {
				fp := filepath.Join(sub, fmt.Sprintf("file_%d_%d.tmp", d, j))
				if err := os.WriteFile(fp, []byte(strings.Repeat("x", 1024)), 0o644); err != nil {
					return err
				}
			}
			if err := createDir(sub, d+1); err != nil {
				return err
			}
		}
		return nil
	}
	return createDir(root, 0)
}

// configCmd — показать текущий конфиг.
func configCmd(args []string) {
	cfg, err := config.Load("")
	if err != nil {
		log.Fatal(err)
	}
	enc := json.NewEncoder(os.Stdout)
	enc.SetIndent("", "  ")
	enc.Encode(cfg)
}
