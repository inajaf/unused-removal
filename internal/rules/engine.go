package rules

import (
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"

	"unused-removal/internal/config"
	"unused-removal/internal/scanner"
)

// Category — категория находки.
type Category string

const (
	CatHuge         Category = "huge"          // > HugeBytes
	CatLarge        Category = "large"         // > LargeBytes
	CatJunk         Category = "junk"          // мусор по паттернам/путям
	CatStale        Category = "stale"         // не менялся > StaleDays
	CatOldLog       Category = "old_log"       // старые логи
	CatStaleInstall Category = "stale_install" // старые инсталляторы
	CatDuplicate    Category = "duplicate"     // дубликат
)

// Risk — уровень риска удаления.
type Risk string

const (
	RiskSafe      Risk = "safe"      // можно удалять безболезненно
	RiskCaution   Risk = "caution"   // лучше проверить
	RiskProtected Risk = "protected" // системные — по умолчанию не предлагаем
)

// Finding — одна находка правила.
type Finding struct {
	Path     string            `json:"path"`
	Size     int64             `json:"size"`
	Category Category          `json:"category"`
	Reason   string            `json:"reason"`
	Risk     Risk              `json:"risk"`
	ModTime  time.Time         `json:"mod_time"`
	Extra    map[string]string `json:"extra,omitempty"` // доп. инфо (для дубликатов: оригинал и т.д.)
}

// Engine — движок правил.
type Engine struct {
	cfg *config.Config
}

func NewEngine(cfg *config.Config) *Engine {
	return &Engine{cfg: cfg}
}

// Analyze прогоняет все правила по списку файлов и возвращает находки.
func (e *Engine) Analyze(recs []scanner.FileRecord) []Finding {
	var findings []Finding
	seen := make(map[string]bool) // защита от дублей правил на одном файле (берём первое сработавшее)

	for _, rec := range recs {
		if rec.Attr.IsDir {
			continue // каталоги не обрабатываем (в будущем — пустые папки)
		}
		// Порядок приоритета: junk > stale > huge/large > duplicate
		// (дубликаты обрабатываются отдельным проходом, см. FindDuplicates)
		if f := e.checkJunk(rec); f != nil && !seen[f.Path] {
			findings = append(findings, *f)
			seen[f.Path] = true
			continue
		}
		if f := e.checkStale(rec); f != nil && !seen[f.Path] {
			findings = append(findings, *f)
			seen[f.Path] = true
			continue
		}
		if f := e.checkLarge(rec); f != nil && !seen[f.Path] {
			findings = append(findings, *f)
			seen[f.Path] = true
			continue
		}
		// duplicate не проверяем здесь — отдельный этап
	}
	return findings
}

// checkJunk — мусорные паттерны и известные мусорные пути.
func (e *Engine) checkJunk(rec scanner.FileRecord) *Finding {
	name := filepath.Base(rec.Path)
	lower := strings.ToLower(name)
	lowerPath := strings.ToLower(rec.Path)

	// 1) Расширения-мусор
	for _, ext := range e.cfg.JunkExtensions {
		if ext == `~$*` {
			if strings.HasPrefix(lower, "~$") {
				return &Finding{Path: rec.Path, Size: rec.Size, Category: CatJunk, Reason: "временный файл Office (~$*)", Risk: RiskSafe, ModTime: rec.ModTime}
			}
			continue
		}
		if strings.HasSuffix(lower, strings.ToLower(ext)) {
			return &Finding{Path: rec.Path, Size: rec.Size, Category: CatJunk, Reason: "расширение " + ext, Risk: RiskSafe, ModTime: rec.ModTime}
		}
	}

	// 2) Известные мусорные каталоги
	for _, jd := range e.cfg.JunkDirs {
		if strings.HasPrefix(lowerPath, strings.ToLower(jd)+`\`) || strings.HasPrefix(lowerPath, strings.ToLower(jd)+`/`) {
			return &Finding{Path: rec.Path, Size: rec.Size, Category: CatJunk, Reason: "в мусорном каталоге: " + jd, Risk: RiskSafe, ModTime: rec.ModTime}
		}
	}

	// 3) Старые логи (отдельная категория для удобства)
	if strings.HasSuffix(lower, ".log") && rec.ModTime.Before(e.cfg.OldLogCutoff()) {
		return &Finding{Path: rec.Path, Size: rec.Size, Category: CatOldLog, Reason: "старый лог (> " + itoa(e.cfg.OldLogDays) + " дней)", Risk: RiskSafe, ModTime: rec.ModTime}
	}

	// 4) Старые инсталляторы в Downloads
	if strings.HasSuffix(lower, ".msi") || strings.HasSuffix(lower, ".exe") || strings.HasSuffix(lower, ".msu") {
		if strings.Contains(lowerPath, `\downloads\`) && rec.ModTime.Before(e.cfg.StaleInstallCutoff()) {
			return &Finding{Path: rec.Path, Size: rec.Size, Category: CatStaleInstall, Reason: "старый инсталлятор в Downloads (> " + itoa(e.cfg.StaleInstallDays) + " дней)", Risk: RiskCaution, ModTime: rec.ModTime}
		}
	}

	return nil
}

// checkStale — файлы, не менявшиеся долгое время.
func (e *Engine) checkStale(rec scanner.FileRecord) *Finding {
	if rec.ModTime.Before(e.cfg.StaleCutoff()) {
		return &Finding{Path: rec.Path, Size: rec.Size, Category: CatStale, Reason: "не менялся > " + itoa(e.cfg.StaleDays) + " дней", Risk: RiskCaution, ModTime: rec.ModTime}
	}
	return nil
}

// checkLarge — крупные файлы (два порога).
func (e *Engine) checkLarge(rec scanner.FileRecord) *Finding {
	if rec.Size >= e.cfg.HugeBytes {
		return &Finding{Path: rec.Path, Size: rec.Size, Category: CatHuge, Reason: "очень крупный файл (> " + formatBytes(e.cfg.HugeBytes) + ")", Risk: RiskCaution, ModTime: rec.ModTime}
	}
	if rec.Size >= e.cfg.LargeBytes {
		return &Finding{Path: rec.Path, Size: rec.Size, Category: CatLarge, Reason: "крупный файл (> " + formatBytes(e.cfg.LargeBytes) + ")", Risk: RiskCaution, ModTime: rec.ModTime}
	}
	return nil
}

// ProtectedPaths — список системных путей, которые по умолчанию защищены от удаления.
// Пути нормализованы к нижнему регистру с прямым слешем.
var ProtectedPaths = []string{
	`c:\windows\winsxs\`,
	`c:\windows\system32\`,
	`c:\windows\syswow64\`,
	`c:\windows\servicing\`,
	`c:\program files\`,
	`c:\program files (x86)\`,
	`c:\pagefile.sys`,
	`c:\hiberfil.sys`,
	`c:\swapfile.sys`,
	`c:\bootmgr`,
	`c:\boot\`,
	`c:\windows\boot\`,
	`c:\recovery\`,
	`c:\system volume information\`,
	`c:\$recycle.bin\`,
}

// IsProtected проверяет, находится ли путь в защищённом списке.
func IsProtected(path string) bool {
	lower := strings.ToLower(path)
	for _, pp := range ProtectedPaths {
		if strings.HasPrefix(lower, pp) {
			return true
		}
	}
	return false
}

// FilterProtected удаляет из списка находки с защищёнными путями (если AllowProtected=false).
func (e *Engine) FilterProtected(findings []Finding) []Finding {
	if e.cfg.AllowProtected {
		return findings
	}
	res := make([]Finding, 0, len(findings))
	for _, f := range findings {
		if IsProtected(f.Path) {
			continue
		}
		// также меняем риск protected -> caution, если он был protected
		if f.Risk == RiskProtected {
			f.Risk = RiskCaution
		}
		res = append(res, f)
	}
	return res
}

// FindDuplicates — отдельный проход поиска дубликатов.
// Группирует файлы по размеру, хэширует только группы с одинаковым размером.
// Хэширование выполняется параллельно (пул = NumCPU) — blake3 на всех ядрах.
// Возвращает находки категории CatDuplicate с полем Extra["original"] = путь к оригиналу.
func (e *Engine) FindDuplicates(recs []scanner.FileRecord) []Finding {
	if !e.cfg.CheckDuplicates {
		return nil
	}
	// 1) Группировка по размеру
	bySize := make(map[int64][]scanner.FileRecord)
	for _, r := range recs {
		if r.Attr.IsDir {
			continue
		}
		if IsProtected(r.Path) {
			continue // защищённые не трогаем
		}
		bySize[r.Size] = append(bySize[r.Size], r)
	}

	// 2) Собираем кандидатов: размер > 0 и минимум 2 файла одного размера.
	var candidates []scanner.FileRecord
	for size, group := range bySize {
		if len(group) < 2 || size == 0 {
			continue
		}
		candidates = append(candidates, group...)
	}
	if len(candidates) < 2 {
		return nil
	}

	// 3) Параллельное хэширование всех кандидатов (CPU-bound hot path).
	type hashed struct {
		rec  scanner.FileRecord
		hash string
	}
	results := make([]hashed, len(candidates))
	workers := runtime.NumCPU()
	if workers > 32 {
		workers = 32
	}
	if workers < 1 {
		workers = 1
	}
	var wg sync.WaitGroup
	ch := make(chan int, workers)
	for w := 0; w < workers; w++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for i := range ch {
				h, err := hashFile(candidates[i].Path)
				if err != nil {
					continue // недоступный файл — пропускаем
				}
				results[i] = hashed{rec: candidates[i], hash: h}
			}
		}()
	}
	for i := range candidates {
		ch <- i
	}
	close(ch)
	wg.Wait()

	// 4) Группировка по хэшу: первый файл хэш-группы — оригинал,
	//    остальные — дубликаты.
	byHash := make(map[string][]hashed)
	for _, r := range results {
		if r.hash == "" {
			continue
		}
		byHash[r.hash] = append(byHash[r.hash], r)
	}

	var findings []Finding
	for _, group := range byHash {
		if len(group) < 2 {
			continue
		}
		first := group[0]
		for _, dup := range group[1:] {
			findings = append(findings, Finding{
				Path:     dup.rec.Path,
				Size:     dup.rec.Size,
				Category: CatDuplicate,
				Reason:   "дубликат файла " + filepath.Base(first.rec.Path),
				Risk:     RiskCaution,
				ModTime:  dup.rec.ModTime,
				Extra:    map[string]string{"original": first.rec.Path},
			})
		}
	}
	return findings
}

// Вспомогательные
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

func formatBytes(b int64) string {
	const unit = 1024
	if b < unit {
		return itoa(int(b)) + " B"
	}
	div, exp := int64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return itoa(int(b/div)) + " " + string("KMGTPE"[exp]) + "iB"
}
