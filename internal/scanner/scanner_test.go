package scanner

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"testing"
	"time"
)

func TestWalker_Basic(t *testing.T) {
	// Создаём временную фикстуру
	tmpDir, err := os.MkdirTemp("", "scanner-test-*")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	// Структура:
	// tmpDir/
	//   file1.txt (100 байт)
	//   subdir/
	//     file2.tmp (200 байт)
	//     file3.log (300 байт)
	//   empty/

	if err := os.WriteFile(filepath.Join(tmpDir, "file1.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	sub := filepath.Join(tmpDir, "subdir")
	if err := os.Mkdir(sub, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sub, "file2.tmp"), []byte("yy"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(sub, "file3.log"), []byte("zzz"), 0o644); err != nil {
		t.Fatal(err)
	}
	empty := filepath.Join(tmpDir, "empty")
	if err := os.Mkdir(empty, 0o755); err != nil {
		t.Fatal(err)
	}

	opts := Options{Workers: runtime.NumCPU()}
	prog := NewProgress()
	w := New(opts, prog, nil)

	ctx := &fakeContext{}
	recs, errs, err := w.Walk(ctx, tmpDir)
	if err != nil {
		t.Fatalf("Walk error: %v", err)
	}
	if len(errs) > 0 {
		t.Errorf("unexpected errors: %v", errs)
	}

	// Должны найти 3 файла (пустые каталоги не считаются)
	if len(recs) != 3 {
		t.Errorf("expected 3 files, got %d: %v", len(recs), recs)
	}

	// Прогресс
	snap := prog.Snapshot()
	if snap.Files != 3 {
		t.Errorf("progress files = %d, want 3", snap.Files)
	}
	if snap.Dirs < 3 {
		t.Errorf("progress dirs = %d, want >= 3", snap.Dirs)
	}
}

func TestWalker_ExcludeDirs(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "scanner-exclude-*")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	// tmpDir/
	//   keep.txt
	//   node_modules/
	//     pkg/
	//       file.js
	//   .git/
	//     config

	if err := os.WriteFile(filepath.Join(tmpDir, "keep.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	nm := filepath.Join(tmpDir, "node_modules", "pkg")
	if err := os.MkdirAll(nm, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(nm, "file.js"), []byte("code"), 0o644); err != nil {
		t.Fatal(err)
	}
	git := filepath.Join(tmpDir, ".git")
	if err := os.MkdirAll(git, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(git, "config"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}

	opts := Options{
		Workers: 2,
		Exclude: []string{"node_modules", ".git"},
	}
	prog := NewProgress()
	w := New(opts, prog, nil)

	ctx := &fakeContext{}
	recs, _, err := w.Walk(ctx, tmpDir)
	if err != nil {
		t.Fatalf("Walk error: %v", err)
	}

	// Должен найти только keep.txt
	if len(recs) != 1 {
		t.Errorf("expected 1 file, got %d: %v", len(recs), recs)
	}
	if recs[0].Path != filepath.Join(tmpDir, "keep.txt") && recs[0].Path != filepath.ToSlash(filepath.Join(tmpDir, "keep.txt")) {
		t.Errorf("unexpected file: %s", recs[0].Path)
	}
}

func TestWalker_ExcludePrefix(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "scanner-expref-*")
	if err != nil {
		t.Fatal(err)
	}
	defer os.RemoveAll(tmpDir)

	// tmpDir/
	//   data/
	//     file1.txt
	//   data2/
	//     file2.txt

	data1 := filepath.Join(tmpDir, "data")
	if err := os.MkdirAll(data1, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(data1, "file1.txt"), []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	data2 := filepath.Join(tmpDir, "data2")
	if err := os.MkdirAll(data2, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(data2, "file2.txt"), []byte("y"), 0o644); err != nil {
		t.Fatal(err)
	}

	opts := Options{
		Workers:     2,
		ExcludePref: []string{filepath.Join(tmpDir, "data") + string(filepath.Separator)},
	}
	prog := NewProgress()
	w := New(opts, prog, nil)

	ctx := &fakeContext{}
	recs, _, err := w.Walk(ctx, tmpDir)
	if err != nil {
		t.Fatalf("Walk error: %v", err)
	}

	t.Logf("ExcludePref: %v", w.opts.ExcludePref)
	t.Logf("Recs: %v", recs)

	// data/ должен быть исключён, data2/ — нет
	if len(recs) != 1 {
		t.Errorf("expected 1 file, got %d: %v", len(recs), recs)
	}
}

// fakeContext — заглушка для context.Context в тестах.
type fakeContext struct{}

func (f *fakeContext) Deadline() (time.Time, bool) { return time.Time{}, false }
func (f *fakeContext) Done() <-chan struct{}       { return nil }
func (f *fakeContext) Err() error                  { return nil }
func (f *fakeContext) Value(key any) any           { return nil }

// makeFixture создаёт дерево с total файлов (по 1 КБ) глубиной depth.
// Используется в бенчмарках производительности сканирования.
func makeFixture(b *testing.B, total, depth int) string {
	root := b.TempDir()
	perDir := total / 100
	if perDir < 1 {
		perDir = 1
	}
	payload := make([]byte, 1024)
	var create func(dir string, d int) error
	create = func(dir string, d int) error {
		if d >= depth {
			return nil
		}
		for i := 0; i < 10; i++ {
			sub := filepath.Join(dir, fmt.Sprintf("d%d_%d", d, i))
			if err := os.MkdirAll(sub, 0o755); err != nil {
				return err
			}
			for j := 0; j < perDir; j++ {
				if err := os.WriteFile(filepath.Join(sub, fmt.Sprintf("f%d_%d", d, j)), payload, 0o644); err != nil {
					return err
				}
			}
			if err := create(sub, d+1); err != nil {
				return err
			}
		}
		return nil
	}
	if err := create(root, 0); err != nil {
		b.Fatal(err)
	}
	return root
}

// BenchmarkWalker_Parallel — производительность параллельного сканирования.
func BenchmarkWalker_Parallel(b *testing.B) {
	root := makeFixture(b, 5000, 3)
	opts := Options{Workers: runtime.NumCPU()}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		prog := NewProgress()
		w := New(opts, prog, nil)
		recs, errs, err := w.Walk(&fakeContext{}, root)
		if err != nil {
			b.Fatal(err)
		}
		if len(recs) == 0 {
			b.Fatal("no records")
		}
		_ = errs
	}
}

// BenchmarkWalker_Serial — последовательный вариант для сравнения.
func BenchmarkWalker_Serial(b *testing.B) {
	root := makeFixture(b, 5000, 3)
	opts := Options{Workers: 1}
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		prog := NewProgress()
		w := New(opts, prog, nil)
		recs, _, err := w.Walk(&fakeContext{}, root)
		if err != nil {
			b.Fatal(err)
		}
		if len(recs) == 0 {
			b.Fatal("no records")
		}
	}
}
