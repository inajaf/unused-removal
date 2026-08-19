// Package scanner — parallel file system walker.
// Walks directories concurrently with a fixed worker pool, caching directory fingerprints
// for incremental scans. Uses raw Win32 stat calls so each entry returns size, mtime and
// attributes in one syscall (no second round-trip). Junction / symbolic links are skipped by
// default to avoid cycles; followLinks can be enabled when needed. Errors during walk are
// recorded but never abort the scan.
//
// Ключевые идеи скорости:
//   - параллельный обход каталогов через общую очередь задач (work queue);
//   - один системный вызов FindFirstFileW возвращает имя, размер, время и
//     атрибуты СРАЗУ для всех записей каталога (без лишних stat на файл);
//   - reparse-точки (junction/symlink) по умолчанию не обходятся — защита от
//     петель и двойного счёта;
//   - инкрементальный кэш отпечатков каталогов (см. Cache): повторные сканы
//     пропускают неизменившиеся поддеревья;
//   - детальные ошибки доступа не прерывают скан (policy: записать и продолжить).
package scanner

import (
	"context"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

// flushBatch — размер локального буфера записей воркера перед сбросом в общий срез.
// Пакетный сброс вместо мьютекса на каждый файл радикально снижает контеншн
// при сканировании сотен тысяч файлов.
const flushBatch = 256

// Attrs — упрощённый набор атрибутов файла, интересный правилам.
type Attrs struct {
	IsDir     bool // каталог
	IsReparse bool // junction/symlink/mount point
	IsHidden  bool
	IsSystem  bool
}

// FileRecord — один файл, увиденный сканером.
type FileRecord struct {
	Path    string
	Size    int64
	ModTime time.Time // время последнего изменения (NTFS, надёжный сигнал)
	Attr    Attrs
}

// ScanError — ошибка при доступе к каталогу/файлу (не фатальна).
type ScanError struct {
	Path string
	Err  error
}

// Options — настройки сканирования.
type Options struct {
	Workers     int      // размер пула воркеров; 0 → NumCPU
	FollowLinks bool     // следовать за reparse-каталогами (с защитой от циклов)
	Exclude     []string // имена каталогов (по компоненту пути), которые не обходить
	ExcludePref []string // префиксы полных путей, которые не обходить
}

// Fingerprint — отпечаток каталога для инкрементального кэша.
type Fingerprint struct {
	ModTimeNS int64 // последнее изменение каталога, нс Unix
}

// CacheEntry — кэшированное содержимое одного каталога.
type CacheEntry struct {
	FP    Fingerprint
	Files []FileRecord // файлы непосредственно в этом каталоге
	Dirs  []string     // имена подкаталогов (их содержимое кэшируется отдельно)
}

// Cache — инкрементальный кэш отпечатков каталогов.
type Cache interface {
	// Lookup возвращает кэшированную запись, если отпечаток совпал.
	Lookup(dir string, fp Fingerprint) (CacheEntry, bool)
	// Save сохраняет запись для каталога.
	Save(dir string, e CacheEntry) error
	// Close закрывает хранилище.
	Close() error
}

// TotalCache — опциональное расширение Cache для хранения общего числа файлов
// прошлого скана (позволяет оценивать процент выполнения в UI).
type TotalCache interface {
	SaveTotal(n int64) error
	LoadTotal() int64
}

// Progress — потокобезопасные счётчики прогресса сканирования.
type Progress struct {
	files    atomic.Int64
	dirs     atomic.Int64
	bytes    atomic.Int64
	errors   atomic.Int64
	total    atomic.Int64 // оценка общего числа файлов (из кэша прошлого скана)
	current  atomic.Value // string
	started  time.Time
	finished atomic.Bool
	cached   atomic.Int64 // каталогов, взятых из кэша

	// Кольцевой буфер последних обработанных файлов (для живой ленты в UI).
	recentMu sync.Mutex
	recent   []string // пути последних обработанных файлов (новые в конец)
}

// recentCap — максимальное число последних файлов в снимке прогресса.
const recentCap = 50

func NewProgress() *Progress {
	p := &Progress{started: time.Now()}
	p.current.Store("")
	return p
}

func (p *Progress) addFile(size int64)  { p.files.Add(1); p.bytes.Add(size) }
func (p *Progress) addDir()             { p.dirs.Add(1) }
func (p *Progress) addError()           { p.errors.Add(1) }
func (p *Progress) addCached()          { p.cached.Add(1) }
func (p *Progress) setCurrent(d string) { p.current.Store(d) }
func (p *Progress) finish()             { p.finished.Store(true) }

// SetTotal задаёт оценку общего числа файлов (из кэша прошлого скана).
// Позволяет вычислять процент выполнения в Snapshot.
func (p *Progress) SetTotal(n int64) { p.total.Store(n) }

// Total возвращает текущую оценку общего числа файлов.
func (p *Progress) Total() int64 { return p.total.Load() }

// addRecentPath добавляет путь в кольцевой буфер последних файлов.
func (p *Progress) addRecentPath(path string) {
	p.recentMu.Lock()
	p.recent = append(p.recent, path)
	if len(p.recent) > recentCap {
		// Удаляем половину старых — амортизированно дёшево.
		copy(p.recent, p.recent[recentCap/2:])
		p.recent = p.recent[:len(p.recent)-recentCap/2]
	}
	p.recentMu.Unlock()
}

// recentPaths возвращает срез последних обработанных файлов.
func (p *Progress) recentPaths() []string {
	p.recentMu.Lock()
	defer p.recentMu.Unlock()
	out := make([]string, len(p.recent))
	copy(out, p.recent)
	return out
}

// Snapshot — срез прогресса для отображения в UI/CLI.
type Snapshot struct {
	Files    int64    `json:"files"`
	Dirs     int64    `json:"dirs"`
	Bytes    int64    `json:"bytes"`
	Errors   int64    `json:"errors"`
	Cached   int64    `json:"cached"`
	Total    int64    `json:"total"`    // оценка общего числа файлов (0 = неизвестно)
	Percent  float64  `json:"percent"`  // процент выполнения (0–100, -1 = неизвестно)
	Current  string   `json:"current"`
	ElapsedS float64  `json:"elapsed_s"`
	RateFPS  float64  `json:"rate_fps"` // файлов в секунду
	RemainS  float64  `json:"remain_s"` // ETA, секунд (0 если не определено)
	Finished bool     `json:"finished"`
	Recent   []string `json:"recent"` // последние обработанные файлы
}

func (p *Progress) Snapshot() Snapshot {
	el := time.Since(p.started).Seconds()
	files := p.files.Load()
	total := p.total.Load()
	s := Snapshot{
		Files: files, Dirs: p.dirs.Load(), Bytes: p.bytes.Load(),
		Errors: p.errors.Load(), Cached: p.cached.Load(),
		Total: total, Percent: -1,
		Current: p.current.Load().(string), ElapsedS: el,
		RateFPS: 0, RemainS: 0, Finished: p.finished.Load(),
		Recent: p.recentPaths(),
	}
	if el > 0.5 && files > 0 {
		s.RateFPS = float64(files) / el
	}
	// Процент и ETA — только если известна оценка общего числа файлов.
	if total > 0 {
		if files > 0 {
			pct := float64(files) / float64(total) * 100
			if pct > 100 {
				pct = 100
			}
			s.Percent = pct
		} else {
			s.Percent = 0
		}
		if s.RateFPS > 0 && files < total {
			s.RemainS = float64(total-files) / s.RateFPS
		}
	}
	if s.Finished {
		s.Percent = 100
	}
	return s
}

// Walker — параллельный обходчик дерева каталогов.
type Walker struct {
	opts     Options
	prog     *Progress
	cache    Cache
	queue    chan string
	wg       sync.WaitGroup
	tasks    sync.WaitGroup // счетчик задач: каталогов в очереди + в обработке
	closer   sync.Once
	errs     []ScanError
	errMu    sync.Mutex
	recs     []FileRecord
	recMu    sync.Mutex
	excludeL []string // Exclude, заранее приведённые к нижнему регистру (без аллокаций в хот-пути)
	prefL    []string // ExcludePref, заранее приведённые к нижнему регистру и обрезанные
	// защита от циклов при FollowLinks (только для reparse-каталогов)
	seenMu sync.Mutex
	seen   map[dirID]struct{}
}

type dirID struct {
	vol uint32 // volume serial number
	idx uint64 // file index (FileIndexHigh<<32 | FileIndexLow)
}

// New создаёт обходчик.
func New(opts Options, prog *Progress, cache Cache) *Walker {
	if opts.Workers <= 0 {
		opts.Workers = runtime.NumCPU()
	}
	if prog == nil {
		prog = NewProgress()
	}
	// Предвычисляем списки исключений в нижнем регистре, чтобы в горячем цикле
	// не делать ToLower на каждый каталог (экономия миллионов аллокаций).
	excludeL := make([]string, 0, len(opts.Exclude))
	for _, e := range opts.Exclude {
		excludeL = append(excludeL, strings.ToLower(e))
	}
	prefL := make([]string, 0, len(opts.ExcludePref))
	for _, p := range opts.ExcludePref {
		prefL = append(prefL, strings.TrimRight(strings.ToLower(p), `\/`))
	}
	return &Walker{
		opts: opts, prog: prog, cache: cache,
		excludeL: excludeL, prefL: prefL,
		seen: map[dirID]struct{}{},
	}
}

// Progress возвращает объект прогресса обходчика.
func (w *Walker) Progress() *Progress { return w.prog }

// flushRecords пакетно переносит локальный буфер воркера в общий срез.
// Один мьютекс на flushBatch записей вместо одного на файл — ключевое
// снижение контеншна при сотнях тысяч файлов.
func (w *Walker) flushRecords(local []FileRecord) {
	if len(local) == 0 {
		return
	}
	w.recMu.Lock()
	w.recs = append(w.recs, local...)
	w.recMu.Unlock()
}

func (w *Walker) addErr(path string, err error) {
	w.errMu.Lock()
	w.errs = append(w.errs, ScanError{Path: path, Err: err})
	w.errMu.Unlock()
	w.prog.addError()
}

// excluded проверяет, нужно ли пропустить каталог (по имени и префиксу).
// Безаллокационный вариант: сравнения идут через EqualFold, префиксы
// предвычислены в нижнем регистре.
func (w *Walker) excluded(dir string) bool {
	// Быстрый путь: исключений нет вовсе.
	if len(w.prefL) == 0 && len(w.excludeL) == 0 {
		return false
	}
	if len(w.prefL) > 0 {
		dirNorm := strings.TrimRight(strings.ToLower(dir), `\/`)
		for _, p := range w.prefL {
			if dirNorm == p || strings.HasPrefix(dirNorm, p+`\`) || strings.HasPrefix(dirNorm, p+`/`) {
				return true
			}
		}
	}
	base := filepath.Base(dir)
	for _, e := range w.excludeL {
		if strings.EqualFold(base, e) {
			return true
		}
	}
	return false
}

// Walk сканирует дерево от корня root и возвращает все записи и ошибки.
func (w *Walker) Walk(ctx context.Context, root string) ([]FileRecord, []ScanError, error) {
	abs, err := filepath.Abs(root)
	if err != nil {
		return nil, nil, err
	}
	abs = filepath.Clean(abs)

	// Оценка общего числа файлов из кэша прошлого скана (для процента в UI).
	if tc, ok := w.cache.(TotalCache); ok {
		if total := tc.LoadTotal(); total > 0 {
			w.prog.SetTotal(total)
		}
	}

	// Очередь с большим буфером, чтобы избежать дедлоков при массовом добавлении подкаталогов.
	// Размер = max(workers * 256, 4096) — обычно достаточно.
	qcap := w.opts.Workers * 256
	if qcap < 4096 {
		qcap = 4096
	}
	w.queue = make(chan string, qcap)

	// Запускаем воркеры
	for i := 0; i < w.opts.Workers; i++ {
		w.wg.Add(1)
		go w.worker(ctx)
	}

	// Корень — первая задача (добавляем СНАЧАЛА, чтобы tasks не был 0)
	w.tasks.Add(1)
	w.queue <- abs

	// Запускаем closer-горутину: ждёт tasks.Wait() и закрывает очередь
	go func() {
		w.tasks.Wait()
		w.closer.Do(func() { close(w.queue) })
	}()

	w.wg.Wait()
	w.prog.finish()

	w.recMu.Lock()
	recs := w.recs
	w.recMu.Unlock()

	// Сохраняем фактическое число файлов для следующего скана (оценка прогресса).
	if tc, ok := w.cache.(TotalCache); ok {
		_ = tc.SaveTotal(int64(len(recs)))
	}

	return recs, w.errs, nil
}

// worker обрабатывает каталоги из очереди.
// Каждый воркер держит собственный буфер записей и сбрасывает его в общий
// срез пакетами — глобальный мьютекс захватывается в flushBatch раз реже.
func (w *Walker) worker(ctx context.Context) {
	defer w.wg.Done()
	local := make([]FileRecord, 0, flushBatch)
	defer func() {
		w.flushRecords(local) // последний неполный пакет
	}()
	for {
		select {
		case <-ctx.Done():
			// отмена: выходим, но оставляем задачи для closer'а
			return
		case dir, ok := <-w.queue:
			if !ok {
				return
			}
			w.processDir(ctx, dir, &local)
			w.tasks.Done() // задача завершена (подкаталоги уже добавлены внутрь)
		}
	}
}

// processDir перечисляет один каталог: проверяет кэш, либо читает с диска.
// Записи накапливаются в локальном буфере local и сбрасываются пакетами.
func (w *Walker) processDir(ctx context.Context, dir string, local *[]FileRecord) {
	w.prog.setCurrent(dir)
	w.prog.addDir()

	// Отпечаток каталога (его собственное время изменения).
	fp, entries, err := readDirEntries(dir)
	if err != nil {
		// ошибка доступа к каталогу — не фатальна, записываем и продолжаем
		w.addErr(dir, err)
		return
	}

	// Инкрементальный кэш: если каталог не менялся и мы его уже сканировали —
	// воспроизводим записи без обхода поддерева.
	if w.cache != nil {
		if ent, ok := w.cache.Lookup(w.cacheKey(dir), fp); ok {
			w.prog.addCached()
			for _, f := range ent.Files {
				*local = append(*local, f)
				if len(*local) >= flushBatch {
					w.flushRecords(*local)
					*local = (*local)[:0]
				}
				w.prog.addFile(f.Size)
				w.prog.addRecentPath(f.Path)
			}
			for _, sub := range ent.Dirs {
				child := dir + "\\" + sub
				if !w.excluded(child) {
					w.tasks.Add(1)
					w.pushDir(ctx, child, local)
				}
			}
			return
		}
	}

	var (
		files   []FileRecord
		subDirs []string
	)
	for _, e := range entries {
		if e.IsDir {
			if e.IsReparse && !w.opts.FollowLinks {
				continue // не следуем за репarse-точками — защита от петель
			}
			child := dir + "\\" + e.Name
			if w.excluded(child) {
				continue
			}
			if e.IsReparse && !w.followCheck(child) {
				continue // цикл через junction/симлинк — уже видели
			}
			subDirs = append(subDirs, e.Name)
			w.tasks.Add(1)
			w.pushDir(ctx, child, local)
			continue
		}
		rec := FileRecord{Path: dir + "\\" + e.Name, Size: e.Size, ModTime: e.ModTime, Attr: e.Attr}
		files = append(files, rec)
		*local = append(*local, rec)
		if len(*local) >= flushBatch {
			w.flushRecords(*local)
			*local = (*local)[:0]
		}
		w.prog.addFile(rec.Size)
		w.prog.addRecentPath(rec.Path)
	}

	if w.cache != nil {
		_ = w.cache.Save(w.cacheKey(dir), CacheEntry{FP: fp, Files: files, Dirs: subDirs})
	}
}

// followCheck возвращает true, если каталог ещё не посещали (по identity).
func (w *Walker) followCheck(path string) bool {
	id, ok := identityOf(path)
	if !ok {
		return true // не смогли узнать identity — доверяем (не блокируем скан)
	}
	w.seenMu.Lock()
	defer w.seenMu.Unlock()
	if _, dup := w.seen[id]; dup {
		return false
	}
	w.seen[id] = struct{}{}
	return true
}

// pushDir ставит каталог в очередь. Если очередь переполнена — обрабатывает
// его СИНХРОННО в текущем воркере. Это критично: иначе все воркеры могут
// одновременно заблокироваться на отправке в заполненный канал (producer-consumer
// deadlock), и сканирование зависнет на каталогах с тысячами подкаталогов.
func (w *Walker) pushDir(ctx context.Context, dir string, local *[]FileRecord) {
	select {
	case <-ctx.Done():
		w.tasks.Done() // отмена: не ставим в очередь
	case w.queue <- dir:
		// успешно поставлен в очередь — воркеры заберут его позже
	default:
		// очередь полна — обрабатываем прямо здесь, чтобы не блокироваться
		w.processDir(ctx, dir, local)
		w.tasks.Done()
	}
}

// cacheKey нормализует путь для использования в качестве ключа кэша.
func (w *Walker) cacheKey(dir string) string {
	return strings.ToLower(filepath.Clean(dir))
}
