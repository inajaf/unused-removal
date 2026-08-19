package rules

import (
	"os"
	"testing"
	"time"

	"unused-removal/internal/config"
	"unused-removal/internal/scanner"
)

func TestEngine_Analyze(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.LargeBytes = 100
	cfg.HugeBytes = 500
	cfg.StaleDays = 10
	cfg.OldLogDays = 5
	cfg.StaleInstallDays = 30
	cfg.JunkExtensions = []string{".tmp", ".bak", "~$*"}
	cfg.JunkDirs = []string{`C:\Temp`, `C:\Windows\Temp`}

	engine := NewEngine(cfg)

	now := time.Now()
	old := now.AddDate(0, 0, -20)
	veryOld := now.AddDate(0, 0, -100)

	recs := []scanner.FileRecord{
		// Junk by extension
		{Path: `C:\Temp\foo.tmp`, Size: 1024, ModTime: now, Attr: scanner.Attrs{}},
		{Path: `C:\Temp\bar.bak`, Size: 2048, ModTime: now, Attr: scanner.Attrs{}},
		{Path: `C:\Temp\doc~$.tmp`, Size: 512, ModTime: now, Attr: scanner.Attrs{}},
		// Junk by directory
		{Path: `C:\Windows\Temp\cleanup.log`, Size: 4096, ModTime: now, Attr: scanner.Attrs{}},
		// Old log
		{Path: `C:\Logs\app.log`, Size: 8192, ModTime: old, Attr: scanner.Attrs{}},
		// Stale
		{Path: `C:\Data\old.txt`, Size: 100, ModTime: veryOld, Attr: scanner.Attrs{}},
		// Large
		{Path: `C:\Data\big.dat`, Size: 200, ModTime: now, Attr: scanner.Attrs{}},
		// Huge
		{Path: `C:\Data\huge.dat`, Size: 1000, ModTime: now, Attr: scanner.Attrs{}},
		// Normal file (should not match)
		{Path: `C:\Data\normal.txt`, Size: 50, ModTime: now, Attr: scanner.Attrs{}},
		// Protected path (should be filtered)
		{Path: `C:\Windows\System32\kernel32.dll`, Size: 1000, ModTime: now, Attr: scanner.Attrs{}},
	}

	findings := engine.Analyze(recs)
	findings = engine.FilterProtected(findings)

	// Проверки
	if len(findings) != 8 {
		t.Fatalf("expected 8 findings, got %d: %+v", len(findings), findings)
	}

	cats := make(map[Category]int)
	for _, f := range findings {
		cats[f.Category]++
	}

	expected := map[Category]int{
		CatJunk:   4, // 3 by ext + 1 by dir
		CatOldLog: 1,
		CatStale:  1,
		CatLarge:  1,
		CatHuge:   1,
	}
	for cat, count := range expected {
		if cats[cat] != count {
			t.Errorf("category %s: expected %d, got %d", cat, count, cats[cat])
		}
	}

	// Проверка, что защищённый путь отфильтрован
	for _, f := range findings {
		if f.Path == `C:\Windows\System32\kernel32.dll` {
			t.Error("protected path should be filtered out")
		}
	}

	// Проверка приоритета: junk должен выигрывать у stale для одного и того же файла
	// (не тестируется здесь напрямую, но логика в Analyze делает continue после первого совпадения)
}

func TestEngine_FindDuplicates(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.CheckDuplicates = true
	engine := NewEngine(cfg)

	dir := t.TempDir()
	write := func(name, content string) string {
		path := dir + "\\" + name
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatal(err)
		}
		return path
	}

	// Два одинаковых файла (1 и 2) и один уникальный (3) одного размера.
	// Проверяем, что дубликат найден, даже если «первый» по порядку уникален.
	p1 := write("a.txt", "identical-content")
	p2 := write("b.txt", "identical-content")
	p3 := write("c.txt", "unique-content")

	recs := []scanner.FileRecord{
		{Path: p1, Size: 16, ModTime: time.Now(), Attr: scanner.Attrs{}},
		{Path: p2, Size: 16, ModTime: time.Now(), Attr: scanner.Attrs{}},
		{Path: p3, Size: 16, ModTime: time.Now(), Attr: scanner.Attrs{}},
	}

	findings := engine.FindDuplicates(recs)

	// Должна быть ровно одна пара дубликатов: p1+p2 (или p2+p1).
	if len(findings) != 1 {
		t.Fatalf("expected 1 duplicate finding, got %d: %+v", len(findings), findings)
	}
	dup := findings[0]
	if dup.Path != p2 && dup.Path != p1 {
		t.Errorf("unexpected duplicate path: %s", dup.Path)
	}
	orig := dup.Extra["original"]
	if orig != p1 && orig != p2 {
		t.Errorf("unexpected original: %s", orig)
	}
	if dup.Category != CatDuplicate {
		t.Errorf("category = %s, want duplicate", dup.Category)
	}
}

func TestIsProtected(t *testing.T) {
	tests := []struct {
		path      string
		protected bool
	}{
		{`C:\Windows\WinSxS\foo.dll`, true},
		{`C:\Windows\System32\drivers\etc\hosts`, true},
		{`C:\Program Files\App\app.exe`, true},
		{`C:\pagefile.sys`, true},
		{`C:\hiberfil.sys`, true},
		{`C:\Users\User\Documents\file.txt`, false},
		{`D:\Projects\code.go`, false},
		{`c:\windows\winsxs\lower.dll`, true}, // case insensitive
	}

	for _, tc := range tests {
		res := IsProtected(tc.path)
		if res != tc.protected {
			t.Errorf("IsProtected(%q) = %v, want %v", tc.path, res, tc.protected)
		}
	}
}

func TestEngine_FilterProtected(t *testing.T) {
	cfg := config.DefaultConfig()
	cfg.AllowProtected = false
	engine := NewEngine(cfg)

	findings := []Finding{
		{Path: `C:\Users\User\file.txt`, Size: 100, Category: CatLarge, Risk: RiskCaution},
		{Path: `C:\Windows\System32\kernel32.dll`, Size: 1000, Category: CatLarge, Risk: RiskProtected},
	}

	filtered := engine.FilterProtected(findings)
	if len(filtered) != 1 {
		t.Errorf("expected 1 finding after filter, got %d", len(filtered))
	}
	if filtered[0].Path != `C:\Users\User\file.txt` {
		t.Errorf("wrong finding kept: %s", filtered[0].Path)
	}
}
