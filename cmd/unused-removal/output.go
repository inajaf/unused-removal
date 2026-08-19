package main

import (
	"fmt"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/mattn/go-isatty"

	"unused-removal/internal/rules"
	"unused-removal/internal/scanner"
)

// isTerminal определяет, подключён ли stdin к интерактивному терминалу.
// Используется, чтобы по умолчанию запускать TUI, а не веб-сервер.
func isTerminal() bool {
	return isatty.IsTerminal(os.Stdin.Fd()) && isatty.IsTerminal(os.Stdout.Fd())
}

// ANSI-цвета для терминала (Windows Terminal / modern console поддерживают).
const (
	ansiReset   = "\x1b[0m"
	ansiBold    = "\x1b[1m"
	ansiDim     = "\x1b[2m"
	ansiCyan    = "\x1b[36m"
	ansiGreen   = "\x1b[32m"
	ansiYellow  = "\x1b[33m"
	ansiRed     = "\x1b[31m"
	ansiMagenta = "\x1b[35m"
	ansiBlue    = "\x1b[34m"
	ansiGray    = "\x1b[90m"
)

// colorEnabled — поддержка ANSI в текущем терминале (WIN32  консоль и *nix tty).
var colorEnabled = detectColor()

func detectColor() bool {
	if os.Getenv("NO_COLOR") != "" {
		return false
	}
	if os.Getenv("TERM") == "dumb" {
		return false
	}
	// Проверяем, что stdout — терминал, а не перенаправление в файл.
	fi, err := os.Stdout.Stat()
	if err != nil {
		return false
	}
	if fi.Mode()&os.ModeCharDevice == 0 {
		return false
	}
	return true
}

func c(code, s string) string {
	if !colorEnabled {
		return s
	}
	return code + s + ansiReset
}

// cliSpinner — простой анимированный спиннер в одной строке (для CLI-прогресса,
// не путать с bubbles/spinner из TUI).
type cliSpinner struct {
	mu     sync.Mutex
	frames []string
	idx    int
}

func newCliSpinner() *cliSpinner {
	return &cliSpinner{frames: []string{"⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"}}
}

func (s *cliSpinner) next() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	f := s.frames[s.idx]
	s.idx = (s.idx + 1) % len(s.frames)
	return f
}

// formatDuration — человекочитаемое представление длительности.
func formatDuration(seconds float64) string {
	if seconds < 60 {
		return fmt.Sprintf("%.1f с", seconds)
	}
	m := int(seconds) / 60
	s := int(seconds) % 60
	if m < 60 {
		return fmt.Sprintf("%d мин %d с", m, s)
	}
	h := m / 60
	return fmt.Sprintf("%d ч %d мин", h, m%60)
}

// printLiveProgress печатает живой прогресс сканирования в одну строку
// (перезаписывая её), пока канал done не закроется.
func printLiveProgress(prog *scanner.Progress, done chan struct{}) {
	if !colorEnabled {
		<-done
		return
	}
	sp := newCliSpinner()
	tick := time.NewTicker(150 * time.Millisecond)
	defer tick.Stop()

	var lastLine string
	clearLine := func() {
		if lastLine != "" {
			fmt.Fprint(os.Stderr, "\r\x1b[2K")
		}
	}
	for {
		select {
		case <-done:
			clearLine()
			return
		case <-tick.C:
			snap := prog.Snapshot()
			var sb strings.Builder
			sb.WriteString(c(ansiCyan, sp.next()))
			sb.WriteString(" ")
			sb.WriteString(c(ansiBold, formatInt(snap.Files)))
			sb.WriteString(" файлов, ")
			sb.WriteString(formatBytes(snap.Bytes))
			if snap.RateFPS > 0 {
				sb.WriteString(" · ")
				sb.WriteString(c(ansiGreen, formatInt(int64(snap.RateFPS))))
				sb.WriteString(" ф/с")
			}
			if snap.Cached > 0 {
				sb.WriteString(" · ")
				sb.WriteString(c(ansiMagenta, formatInt(snap.Cached)))
				sb.WriteString(" из кэша")
			}
			if snap.Current != "" {
				sb.WriteString(" · ")
				sb.WriteString(c(ansiGray, truncate(snap.Current, 60)))
			}
			line := sb.String()
			if line != lastLine {
				clearLine()
				fmt.Fprint(os.Stderr, line)
				lastLine = line
			}
		}
	}
}

// printHeader — цветная шапка команды.
func printHeader(title string) {
	fmt.Println()
	fmt.Println(c(ansiBold+ansiCyan, "▸ "+title))
	fmt.Println(c(ansiGray, "  "+strings.Repeat("─", 56)))
}

// printSummaryTable — красивая сводка по категориям.
func printSummaryTable(findings []rules.Finding) {
	byCat := make(map[rules.Category]struct {
		count int
		bytes int64
	})
	var order []rules.Category
	for _, f := range findings {
		if _, ok := byCat[f.Category]; !ok {
			order = append(order, f.Category)
		}
		e := byCat[f.Category]
		e.count++
		e.bytes += f.Size
		byCat[f.Category] = e
	}

	if len(order) == 0 {
		fmt.Println(c(ansiGray, "  Ничего не найдено — отличная новость!"))
		return
	}

	fmt.Println(c(ansiBold, "  Категория") + c(ansiDim, "          файлов   размер"))
	for _, cat := range order {
		e := byCat[cat]
		icon := categoryIconCLI(cat)
		name := categoryNameCLI(cat)
		fmt.Printf("  %s %-20s %7d   %s\n",
			icon, c(categoryColorCLI(cat), name), e.count, formatBytes(e.bytes))
	}
}

// printTopFindings — топ N находок по размеру.
func printTopFindings(findings []rules.Finding, n int) {
	if len(findings) == 0 {
		return
	}
	if n > len(findings) {
		n = len(findings)
	}
	fmt.Println(c(ansiBold, "\n  Крупнейшие находки:"))
	for i := 0; i < n; i++ {
		f := findings[i]
		fmt.Printf("  %s %10s  %s\n",
			c(categoryColorCLI(f.Category), categoryIconCLI(f.Category)),
			c(ansiBold, formatBytes(f.Size)),
			c(ansiGray, f.Path))
	}
}

func categoryIconCLI(cat rules.Category) string {
	switch cat {
	case rules.CatHuge:
		return "🔴"
	case rules.CatLarge:
		return "🟠"
	case rules.CatJunk:
		return "🗑"
	case rules.CatOldLog:
		return "📄"
	case rules.CatStaleInstall:
		return "📦"
	case rules.CatStale:
		return "⏳"
	case rules.CatDuplicate:
		return "🔁"
	default:
		return "📁"
	}
}

func categoryNameCLI(cat rules.Category) string {
	switch cat {
	case rules.CatHuge:
		return "очень крупные"
	case rules.CatLarge:
		return "крупные"
	case rules.CatJunk:
		return "мусор"
	case rules.CatOldLog:
		return "старые логи"
	case rules.CatStaleInstall:
		return "старые инсталляторы"
	case rules.CatStale:
		return "не использовались"
	case rules.CatDuplicate:
		return "дубликаты"
	default:
		return string(cat)
	}
}

func categoryColorCLI(cat rules.Category) string {
	switch cat {
	case rules.CatHuge:
		return ansiRed
	case rules.CatLarge:
		return ansiYellow
	case rules.CatJunk:
		return ansiGreen
	case rules.CatOldLog:
		return ansiBlue
	case rules.CatStaleInstall:
		return ansiMagenta
	case rules.CatStale:
		return ansiGray
	case rules.CatDuplicate:
		return ansiYellow
	default:
		return ansiGray
	}
}

// formatInt — форматирование с разделителями тысяч.
func formatInt(n int64) string {
	s := fmt.Sprintf("%d", n)
	if n < 0 {
		s = s[1:]
	}
	var sb strings.Builder
	for i, ch := range s {
		if i > 0 && (len(s)-i)%3 == 0 {
			sb.WriteByte(' ')
		}
		sb.WriteRune(ch)
	}
	if n < 0 {
		return "-" + sb.String()
	}
	return sb.String()
}

func truncate(s string, n int) string {
	if len(s) <= n {
		return s
	}
	return s[:n-1] + "…"
}
