package ui

import (
	"context"
	"embed"
	"encoding/csv"
	"encoding/json"
	"log"
	"net"
	"net/http"
	"os/exec"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"unused-removal/internal/cleaner"
	"unused-removal/internal/config"
	"unused-removal/internal/rules"
	"unused-removal/internal/scanner"
)

//go:embed web
var webFS embed.FS

// Server — HTTP-сервер для веб-интерфейса.
type Server struct {
	cfg      *config.Config
	scanner  *scanner.Walker
	prog     *scanner.Progress
	cache    scanner.Cache
	results  []rules.Finding
	recs     []scanner.FileRecord
	errs     []scanner.ScanError
	scanMu   sync.Mutex
	scanID   int
	scanDone bool
	cancel   context.CancelFunc // отмена текущего сканирования
}

// NewServer создаёт новый сервер.
func NewServer(cfg *config.Config) *Server {
	return &Server{cfg: cfg}
}

// Start запускает HTTP-сервер.
func (s *Server) Start(addr string) error {
	mux := http.NewServeMux()

	// API
	mux.HandleFunc("/api/scan", s.handleScan)
	mux.HandleFunc("/api/stop", s.handleStop)
	mux.HandleFunc("/api/progress", s.handleProgress)
	mux.HandleFunc("/api/results", s.handleResults)
	mux.HandleFunc("/api/delete", s.handleDelete)
	mux.HandleFunc("/api/config", s.handleConfig)
	mux.HandleFunc("/api/export", s.handleExport)

	// Статические файлы из embed.FS (файлы лежат в web/ подкаталоге)
	staticHandler := func(w http.ResponseWriter, r *http.Request) {
		path := r.URL.Path
		if path == "/" {
			path = "/index.html"
		}
		// В embed.FS файлы лежат под web/
		embedPath := "web" + path
		data, err := webFS.ReadFile(embedPath)
		if err != nil {
			http.Error(w, "not found", http.StatusNotFound)
			return
		}
		// Content-Type
		switch {
		case strings.HasSuffix(path, ".css"):
			w.Header().Set("Content-Type", "text/css; charset=utf-8")
		case strings.HasSuffix(path, ".js"):
			w.Header().Set("Content-Type", "application/javascript; charset=utf-8")
		case strings.HasSuffix(path, ".html"):
			w.Header().Set("Content-Type", "text/html; charset=utf-8")
		}
		w.Write(data)
	}
	mux.HandleFunc("/", staticHandler)

	// Если порт 0 — даём ОС выбрать свободный
	listener, err := net.Listen("tcp", addr)
	if err != nil {
		return err
	}
	actualAddr := listener.Addr().String()
	log.Printf("Веб-интерфейс запущен на http://%s", actualAddr)

	// Пытаемся открыть браузер
	go s.openBrowser("http://" + actualAddr)

	return http.Serve(listener, mux)
}

func (s *Server) openBrowser(url string) {
	// Небольшая задержка, чтобы сервер точно успел запуститься
	time.Sleep(500 * time.Millisecond)
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("rundll32", "url.dll,FileProtocolHandler", url)
	case "darwin":
		cmd = exec.Command("open", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}
	_ = cmd.Start()
}

func (s *Server) handleScan(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		Root            string `json:"root"`
		Workers         int    `json:"workers"`
		FollowLinks     bool   `json:"follow_links"`
		UseCache        bool   `json:"use_cache"`
		CheckDuplicates bool   `json:"check_duplicates"`
		ProtectSystem   bool   `json:"protect_system"`
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	// Применяем настройки
	cfg := *s.cfg
	if req.Root != "" {
		cfg.Root = req.Root
	}
	if req.Workers > 0 {
		cfg.Workers = req.Workers
	}
	cfg.FollowLinks = req.FollowLinks
	cfg.UseCache = req.UseCache
	cfg.CheckDuplicates = req.CheckDuplicates
	cfg.ProtectSystem = req.ProtectSystem

	// Запускаем скан в фоне
	s.scanMu.Lock()
	s.scanID++
	scanID := s.scanID
	s.results = nil
	s.recs = nil
	s.errs = nil
	s.scanDone = false
	if s.cancel != nil {
		s.cancel() // отменяем предыдущее сканирование, если оно ещё шло
	}
	ctx, cancel := context.WithCancel(context.Background())
	s.cancel = cancel
	s.prog = scanner.NewProgress()
	s.scanMu.Unlock()

	go func() {
		defer cancel()
		opts := *cfg.ScannerOptions()
		var cache scanner.Cache
		if cfg.UseCache {
			hash := configHash(&cfg)
			c, err := scanner.NewBoltCache("unused-removal", hash)
			if err == nil {
				cache = c
				defer c.Close()
			}
		}
		w := scanner.New(opts, s.prog, cache)

		recs, errs, err := w.Walk(ctx, cfg.Root)
		if err != nil {
			log.Printf("scan error: %v", err)
		}

		engine := rules.NewEngine(&cfg)
		findings := engine.Analyze(recs)
		findings = engine.FilterProtected(findings)
		if cfg.CheckDuplicates {
			dups := engine.FindDuplicates(recs)
			findings = append(findings, dups...)
		}

		// Сортировка по убыванию размера
		sort.Slice(findings, func(i, j int) bool {
			return findings[i].Size > findings[j].Size
		})

		s.scanMu.Lock()
		s.recs = recs
		s.errs = errs
		s.results = findings
		s.scanDone = true
		s.scanMu.Unlock()
	}()

	json.NewEncoder(w).Encode(map[string]any{
		"scan_id": scanID,
		"status":  "started",
	})
}

// handleStop отменяет текущее сканирование.
func (s *Server) handleStop(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	s.scanMu.Lock()
	stopped := false
	if s.cancel != nil && !s.scanDone {
		s.cancel()
		stopped = true
	}
	s.scanMu.Unlock()

	json.NewEncoder(w).Encode(map[string]any{
		"status":  "stopped",
		"stopped": stopped,
	})
}

func (s *Server) handleProgress(w http.ResponseWriter, r *http.Request) {
	s.scanMu.Lock()
	prog := s.prog
	done := s.scanDone
	s.scanMu.Unlock()

	if prog == nil {
		http.Error(w, "no scan in progress", http.StatusBadRequest)
		return
	}

	snap := prog.Snapshot()
	json.NewEncoder(w).Encode(map[string]any{
		"progress": snap,
		"done":     done,
	})
}

func (s *Server) handleResults(w http.ResponseWriter, r *http.Request) {
	s.scanMu.Lock()
	findings := s.results
	s.scanMu.Unlock()

	if findings == nil {
		findings = []rules.Finding{}
	}

	// Фильтрация по query параметрам
	category := r.URL.Query().Get("category")
	search := strings.ToLower(r.URL.Query().Get("search"))
	limit := r.URL.Query().Get("limit")
	offset := r.URL.Query().Get("offset")

	filtered := findings
	if category != "" {
		filtered = filterByCategory(filtered, rules.Category(category))
	}
	if search != "" {
		filtered = filterBySearch(filtered, search)
	}

	// Пагинация
	off, _ := strconv.Atoi(offset)
	lim, _ := strconv.Atoi(limit)
	if lim <= 0 {
		lim = len(filtered)
	}
	if off < 0 {
		off = 0
	}
	if off >= len(filtered) {
		filtered = []rules.Finding{}
	} else {
		end := off + lim
		if end > len(filtered) {
			end = len(filtered)
		}
		filtered = filtered[off:end]
	}

	json.NewEncoder(w).Encode(map[string]any{
		"total":    len(findings),
		"filtered": len(filtered),
		"items":    filtered,
	})
}

func filterByCategory(findings []rules.Finding, cat rules.Category) []rules.Finding {
	res := make([]rules.Finding, 0, len(findings))
	for _, f := range findings {
		if f.Category == cat {
			res = append(res, f)
		}
	}
	return res
}

func filterBySearch(findings []rules.Finding, search string) []rules.Finding {
	res := make([]rules.Finding, 0, len(findings))
	for _, f := range findings {
		if strings.Contains(strings.ToLower(f.Path), search) ||
			strings.Contains(strings.ToLower(f.Reason), search) {
			res = append(res, f)
		}
	}
	return res
}

func (s *Server) handleDelete(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}

	var req struct {
		Paths []string `json:"paths"`
		Mode  string   `json:"mode"` // "recycle" или "hard"
	}
	if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	if len(req.Paths) == 0 {
		http.Error(w, "no paths provided", http.StatusBadRequest)
		return
	}

	var res cleaner.Result
	var err error
	if req.Mode == "hard" {
		res, err = cleaner.HardDelete(req.Paths)
	} else {
		res, err = cleaner.RecycleBin(req.Paths)
	}

	json.NewEncoder(w).Encode(map[string]any{
		"deleted":     len(res.Deleted),
		"failed":      len(res.Failed),
		"total_bytes": res.TotalBytes,
		"errors":      res.Failed,
		"success":     err == nil,
	})
}

func (s *Server) handleConfig(w http.ResponseWriter, r *http.Request) {
	switch r.Method {
	case http.MethodGet:
		json.NewEncoder(w).Encode(s.cfg)
	case http.MethodPut:
		var cfg config.Config
		if err := json.NewDecoder(r.Body).Decode(&cfg); err != nil {
			http.Error(w, err.Error(), http.StatusBadRequest)
			return
		}
		*s.cfg = cfg
		// Сохраняем в файл
		if err := s.cfg.Save("config.toml"); err != nil {
			http.Error(w, err.Error(), http.StatusInternalServerError)
			return
		}
		json.NewEncoder(w).Encode(map[string]string{"status": "saved"})
	default:
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
	}
}

func (s *Server) handleExport(w http.ResponseWriter, r *http.Request) {
	s.scanMu.Lock()
	findings := s.results
	s.scanMu.Unlock()

	if findings == nil {
		http.Error(w, "no scan results", http.StatusBadRequest)
		return
	}

	format := r.URL.Query().Get("format")
	if format == "" {
		format = "json"
	}

	if format == "csv" {
		w.Header().Set("Content-Type", "text/csv")
		w.Header().Set("Content-Disposition", `attachment; filename="unused-removal-report.csv"`)
		writeCSV(w, findings)
	} else {
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Content-Disposition", `attachment; filename="unused-removal-report.json"`)
		json.NewEncoder(w).Encode(findings)
	}
}

func writeCSV(w http.ResponseWriter, findings []rules.Finding) {
	cw := csv.NewWriter(w)
	defer cw.Flush()
	cw.Write([]string{"path", "size_bytes", "category", "reason", "risk", "mod_time"})
	for _, f := range findings {
		cw.Write([]string{
			f.Path,
			strconv.FormatInt(f.Size, 10),
			string(f.Category),
			f.Reason,
			string(f.Risk),
			f.ModTime.Format(time.RFC3339),
		})
	}
}

// configHash — копия из scanner для инвалидации кэша.
func configHash(cfg *config.Config) string {
	var sb strings.Builder
	sb.WriteString("w:")
	sb.WriteString(strconv.Itoa(cfg.Workers))
	sb.WriteString(" fl:")
	sb.WriteString(strconv.Itoa(boolToInt(cfg.FollowLinks)))
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
	return strconv.FormatUint(h, 16)
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}
