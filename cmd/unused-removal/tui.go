package main

import (
	"context"
	"fmt"
	"os"
	"strings"
	"time"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/spinner"
	"charm.land/bubbles/v2/table"
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"unused-removal/internal/cleaner"
	"unused-removal/internal/config"
	"unused-removal/internal/rules"
	"unused-removal/internal/scanner"
)

// tuiCmd — запуск интерактивного терминального интерфейса.
func tuiCmd(args []string) {
	cfg, err := config.Load("")
	if err != nil {
		fmt.Fprintf(os.Stderr, "config load: %v\n", err)
		os.Exit(1)
	}
	m := newTuiModel(cfg)
	p := tea.NewProgram(m)
	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "tui error: %v\n", err)
		os.Exit(1)
	}
}

// ===== Состояние и модель =====

type tuiState int

const (
	stateConfig tuiState = iota
	stateScanning
	stateResults
)

type tuiModel struct {
	state tuiState
	cfg   *config.Config

	// Сканирование
	prog     *scanner.Progress
	cancel   context.CancelFunc
	findings []rules.Finding

	// Каналы для передачи результата из фоновой горутины в Update
	scanCh chan scanDoneMsg
	delCh  chan deleteDoneMsg

	// UI: конфиг
	rootInput    textinput.Model
	workersInput textinput.Model
	followLinks  bool
	useCache     bool
	checkDup     bool
	protectSys   bool
	cfgFocus     int // индекс активного поля в форме

	// UI: прогресс
	spinner spinner.Model

	// UI: результаты
	table      table.Model
	filterCat  string
	search     string
	selected   map[string]bool
	help       help.Model
	statusMsg  string
	statusType string // "ok" | "err" | "info"

	width, height int
	quitting      bool
}

func newTuiModel(cfg *config.Config) *tuiModel {
	root := textinput.New()
	root.Placeholder = `C:\`
	root.SetValue(cfg.Root)
	root.CharLimit = 260
	root.SetWidth(40)

	workers := textinput.New()
	workers.Placeholder = "0 = авто"
	workers.SetValue("0")
	workers.CharLimit = 4
	workers.SetWidth(8)

	sp := spinner.New(spinner.WithSpinner(spinner.Dot))

	cols := []table.Column{
		{Title: "Размер", Width: 12},
		{Title: "Категория", Width: 20},
		{Title: "Риск", Width: 10},
		{Title: "Путь", Width: 80},
	}
	styles := table.Styles{
		Header: lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#a5b4fc")).
			Padding(0, 1),
		Selected: lipgloss.NewStyle().
			Background(lipgloss.Color("#3b4252")).
			Foreground(lipgloss.Color("#e5e9f0")).
			Bold(true),
		Cell: lipgloss.NewStyle().
			Padding(0, 1).
			Foreground(lipgloss.Color("#d8dee9")),
	}
	tbl := table.New(
		table.WithColumns(cols),
		table.WithRows([]table.Row{}),
		table.WithFocused(true),
		table.WithHeight(20),
		table.WithStyles(styles),
	)

	return &tuiModel{
		cfg:          cfg,
		rootInput:    root,
		workersInput: workers,
		followLinks:  cfg.FollowLinks,
		useCache:     cfg.UseCache,
		checkDup:     cfg.CheckDuplicates,
		protectSys:   cfg.ProtectSystem,
		spinner:      sp,
		table:        tbl,
		selected:     map[string]bool{},
		help:         help.New(),
	}
}

// ===== Init / Update / View =====

func (m *tuiModel) Init() tea.Cmd {
	return nil
}

type progressTickMsg struct{}

func progressTick() tea.Cmd {
	return tea.Tick(150*time.Millisecond, func(time.Time) tea.Msg { return progressTickMsg{} })
}

type scanDoneMsg struct {
	recs     []scanner.FileRecord
	errs     []scanner.ScanError
	err      error
	findings []rules.Finding
}

type deleteDoneMsg struct {
	res   cleaner.Result
	err   error
	mode  string
	paths []string
}

func (m *tuiModel) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.table.SetHeight(max(10, msg.Height-14))
		m.table.SetWidth(msg.Width - 4)
		return m, nil

	case tea.KeyPressMsg:
		switch m.state {
		case stateConfig:
			return m.updateConfigKey(msg)
		case stateScanning:
			return m.updateScanningKey(msg)
		case stateResults:
			return m.updateResultsKey(msg)
		}

	case progressTickMsg:
		if m.state == stateScanning {
			// Скан завершился? Читаем канал неблокирующе.
			select {
			case done := <-m.scanCh:
				m.state = stateResults
				m.findings = done.findings
				if done.err != nil {
					m.statusMsg = "Ошибка сканирования: " + done.err.Error()
					m.statusType = "err"
				} else {
					m.statusMsg = fmt.Sprintf("Найдено %d файлов, %d находок", len(done.recs), len(done.findings))
					m.statusType = "ok"
				}
				m.rebuildTable()
				return m, nil
			default:
			}
		}
		// Проверяем канал удаления (из любого состояния)
		if m.delCh != nil {
			select {
			case del := <-m.delCh:
				m.delCh = nil
				return m, m.applyDelete(del)
			default:
			}
		}
		return m, progressTick()

	case scanDoneMsg:
		m.state = stateResults
		m.findings = msg.findings
		if msg.err != nil {
			m.statusMsg = "Ошибка сканирования: " + msg.err.Error()
			m.statusType = "err"
		} else {
			m.statusMsg = fmt.Sprintf("Найдено %d файлов, %d находок", len(msg.recs), len(msg.findings))
			m.statusType = "ok"
		}
		m.rebuildTable()
		return m, nil

	case deleteDoneMsg:
		return m, m.applyDelete(msg)

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	}
	return m, nil
}

// applyDelete применяет результат удаления: обновляет статус и список находок.
func (m *tuiModel) applyDelete(msg deleteDoneMsg) tea.Cmd {
	m.statusType = "ok"
	if msg.err != nil {
		m.statusType = "err"
		m.statusMsg = "Ошибка удаления: " + msg.err.Error()
	} else {
		what := "в Корзину"
		if msg.mode == "hard" {
			what = "безвозвратно"
		}
		m.statusMsg = fmt.Sprintf("Удалено %d из %d файлов %s (%s)",
			len(msg.res.Deleted), len(msg.paths), what, formatBytes(msg.res.TotalBytes))
	}
	// Убираем удалённые из списка
	deletedSet := make(map[string]bool, len(msg.res.Deleted))
	for _, p := range msg.res.Deleted {
		deletedSet[p] = true
	}
	var keep []rules.Finding
	for _, f := range m.findings {
		if !deletedSet[f.Path] {
			keep = append(keep, f)
		}
	}
	m.findings = keep
	for _, p := range msg.res.Deleted {
		delete(m.selected, p)
	}
	m.rebuildTable()
	return nil
}

// ===== Клавиши: конфиг =====

func (m *tuiModel) updateConfigKey(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c", "q":
		m.quitting = true
		return m, tea.Quit
	case "tab", "down":
		m.cfgFocus = (m.cfgFocus + 1) % 5
		return m, nil
	case "shift+tab", "up":
		m.cfgFocus = (m.cfgFocus + 4) % 5
		return m, nil
	case "enter":
		if m.cfgFocus == 4 { // кнопка «Сканировать»
			return m.startScan()
		}
		// Enter на текстовом поле — переход к следующему
		m.cfgFocus = (m.cfgFocus + 1) % 5
		return m, nil
	case " ", "space":
		switch m.cfgFocus {
		case 2:
			m.followLinks = !m.followLinks
		case 3:
			m.useCache = !m.useCache
		case 4:
			// последний элемент — кнопка; пробел тоже запускает скан
			return m.startScan()
		}
		return m, nil
	}

	// Текстовые поля
	if m.cfgFocus == 0 {
		m.rootInput, _ = m.rootInput.Update(msg)
	} else if m.cfgFocus == 1 {
		m.workersInput, _ = m.workersInput.Update(msg)
	}
	return m, nil
}

// ===== Клавиши: сканирование =====

func (m *tuiModel) updateScanningKey(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c", "q":
		if m.cancel != nil {
			m.cancel()
		}
		m.quitting = true
		return m, tea.Quit
	case "s":
		if m.cancel != nil {
			m.cancel()
		}
		m.statusMsg = "Сканирование остановлено"
		m.statusType = "info"
		m.state = stateConfig
		return m, nil
	}
	return m, nil
}

// ===== Клавиши: результаты =====

func (m *tuiModel) updateResultsKey(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "ctrl+c", "q":
		m.quitting = true
		return m, tea.Quit
	case "r":
		// назад к настройкам
		m.state = stateConfig
		m.statusMsg = ""
		return m, nil
	case "t":
		return m.deleteSelected("recycle")
	case "x":
		return m.deleteSelected("hard")
	case "/":
		// упрощённый поиск: показываем подсказку
		m.statusMsg = "Поиск: используйте клавиши c/g для фильтра по категории"
		m.statusType = "info"
		return m, nil
	case "c", "g":
		// цикл по категориям
		cats := []string{"", "huge", "large", "junk", "old_log", "stale_install", "stale", "duplicate"}
		idx := 0
		for i, c := range cats {
			if c == m.filterCat {
				idx = i
				break
			}
		}
		m.filterCat = cats[(idx+1)%len(cats)]
		m.rebuildTable()
		return m, nil
	case " ", "space":
		if len(m.table.Rows()) > 0 {
			row := m.table.SelectedRow()
			if len(row) > 3 {
				path := row[3]
				m.selected[path] = !m.selected[path]
				m.statusMsg = fmt.Sprintf("Выбрано: %d", len(m.selected))
				m.statusType = "info"
			}
		}
		return m, nil
	}

	var cmd tea.Cmd
	m.table, cmd = m.table.Update(msg)
	return m, cmd
}

// ===== Действия =====

func (m *tuiModel) startScan() (tea.Model, tea.Cmd) {
	root := strings.TrimSpace(m.rootInput.Value())
	if root == "" {
		m.statusMsg = "Укажите корневой путь"
		m.statusType = "err"
		return m, nil
	}

	// Настройки
	cfg := *m.cfg
	cfg.Root = root
	cfg.FollowLinks = m.followLinks
	cfg.UseCache = m.useCache
	cfg.CheckDuplicates = m.checkDup
	cfg.ProtectSystem = m.protectSys
	if w := strings.TrimSpace(m.workersInput.Value()); w != "" {
		fmt.Sscanf(w, "%d", &cfg.Workers)
	}

	m.state = stateScanning
	m.statusMsg = ""
	m.prog = scanner.NewProgress()
	m.findings = nil

	ctx, cancel := context.WithCancel(context.Background())
	m.cancel = cancel
	m.scanCh = make(chan scanDoneMsg, 1)

	// Фоновое сканирование; результат — в канал, который опрашивает progressTick.
	go func() {
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
		w := scanner.New(opts, m.prog, cache)
		recs, _, err := w.Walk(ctx, cfg.Root)

		var findings []rules.Finding
		if err == nil {
			engine := rules.NewEngine(&cfg)
			findings = engine.Analyze(recs)
			findings = engine.FilterProtected(findings)
			if cfg.CheckDuplicates {
				findings = append(findings, engine.FindDuplicates(recs)...)
			}
			sortBySizeDesc(findings)
		}
		m.scanCh <- scanDoneMsg{recs: recs, err: err, findings: findings}
	}()

	return m, tea.Batch(m.spinner.Tick, progressTick())
}

// deleteSelected удаляет выбранные файлы.
func (m *tuiModel) deleteSelected(mode string) (tea.Model, tea.Cmd) {
	if len(m.selected) == 0 {
		m.statusMsg = "Ничего не выбрано — нажмите Пробел на строке"
		m.statusType = "info"
		return m, nil
	}
	paths := make([]string, 0, len(m.selected))
	for p := range m.selected {
		paths = append(paths, p)
	}

	m.delCh = make(chan deleteDoneMsg, 1)
	go func() {
		var res cleaner.Result
		var err error
		if mode == "hard" {
			res, err = cleaner.HardDelete(paths)
		} else {
			res, err = cleaner.RecycleBin(paths)
		}
		m.delCh <- deleteDoneMsg{res: res, err: err, mode: mode, paths: paths}
	}()

	return m, progressTick() // опрос канала
}

// ===== Таблица =====

func (m *tuiModel) rebuildTable() {
	var rows []table.Row
	for _, f := range m.findings {
		if m.filterCat != "" && f.Category != rules.Category(m.filterCat) {
			continue
		}
		mark := " "
		if m.selected[f.Path] {
			mark = "●"
		}
		rows = append(rows, table.Row{
			mark + " " + formatBytes(f.Size),
			categoryNameCLI(f.Category),
			string(f.Risk),
			f.Path,
		})
	}
	m.table.SetRows(rows)
}

func sortBySizeDesc(f []rules.Finding) {
	// пузырьковая сортировка не нужна — используем sort
	for i := 1; i < len(f); i++ {
		for j := i; j > 0 && f[j].Size > f[j-1].Size; j-- {
			f[j], f[j-1] = f[j-1], f[j]
		}
	}
}

// ===== View =====

var (
	titleStyle = lipgloss.NewStyle().
			Bold(true).
			Foreground(lipgloss.Color("#a5b4fc")).
			Padding(0, 1)

	panelStyle = lipgloss.NewStyle().
			Border(lipgloss.RoundedBorder()).
			BorderForeground(lipgloss.Color("#2e3440")).
			Padding(1, 2)

	labelStyle = lipgloss.NewStyle().
			Foreground(lipgloss.Color("#8f9bb3")).
			Bold(true)

	okStyle   = lipgloss.NewStyle().Foreground(lipgloss.Color("#a3be8c"))
	errStyle  = lipgloss.NewStyle().Foreground(lipgloss.Color("#bf616a"))
	infoStyle = lipgloss.NewStyle().Foreground(lipgloss.Color("#88c0d0"))
	dimStyle  = lipgloss.NewStyle().Foreground(lipgloss.Color("#4c566a"))
)

func (m *tuiModel) View() tea.View {
	if m.quitting {
		v := tea.NewView("Спасибо за использование unused-removal!\n")
		return v
	}

	var content string
	switch m.state {
	case stateConfig:
		content = m.viewConfig()
	case stateScanning:
		content = m.viewScanning()
	case stateResults:
		content = m.viewResults()
	}
	v := tea.NewView(content)
	v.AltScreen = true // полноэкранный режим (аналог WithAltScreen в v1)
	return v
}

func (m *tuiModel) viewConfig() string {
	var b strings.Builder
	b.WriteString(titleStyle.Render("🗂 unused-removal"))
	b.WriteString("\n\n")
	b.WriteString(panelStyle.Render(
		m.renderConfigForm(),
	))
	if m.statusMsg != "" {
		b.WriteString("\n\n" + m.renderStatus())
	}
	b.WriteString("\n\n")
	b.WriteString(m.renderHelp("Конфигурация"))
	return b.String()
}

func (m *tuiModel) renderConfigForm() string {
	fieldStyle := lipgloss.NewStyle().Foreground(lipgloss.Color("#d8dee9"))
	activeFieldStyle := lipgloss.NewStyle().
		Foreground(lipgloss.Color("#d8dee9")).
		BorderLeft(true).
		BorderStyle(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#a5b4fc")).
		PaddingLeft(1)

	var b strings.Builder
	b.WriteString(labelStyle.Render("Корневой путь"))
	b.WriteString("\n")
	if m.cfgFocus == 0 {
		b.WriteString(activeFieldStyle.Render(m.rootInput.View()))
	} else {
		b.WriteString(fieldStyle.Render(m.rootInput.Value()))
	}
	b.WriteString("\n\n")

	b.WriteString(labelStyle.Render("Потоки (0 = авто)"))
	b.WriteString("\n")
	if m.cfgFocus == 1 {
		b.WriteString(activeFieldStyle.Render(m.workersInput.View()))
	} else {
		b.WriteString(fieldStyle.Render(m.workersInput.Value()))
	}
	b.WriteString("\n\n")

	b.WriteString(m.renderCheckbox(2, "Следовать за junction/symlink", m.followLinks))
	b.WriteString("\n")
	b.WriteString(m.renderCheckbox(3, "Инкрементальный кэш", m.useCache))
	b.WriteString("\n")
	b.WriteString(m.renderCheckbox(4, "Поиск дубликатов (медленно)", m.checkDup))
	b.WriteString("\n\n")

	// Кнопка
	btnStyle := lipgloss.NewStyle().
		Bold(true).
		Foreground(lipgloss.Color("#1a1b26")).
		Background(lipgloss.Color("#a5b4fc")).
		Padding(0, 2)
	if m.cfgFocus == 5 {
		btnStyle = btnStyle.Background(lipgloss.Color("#a3be8c"))
	}
	b.WriteString(btnStyle.Render("▶ Начать сканирование"))
	return b.String()
}

func (m *tuiModel) renderCheckbox(focus int, label string, checked bool) string {
	box := "[ ]"
	if checked {
		box = "[✓]"
	}
	style := lipgloss.NewStyle().Foreground(lipgloss.Color("#d8dee9"))
	if m.cfgFocus == focus {
		style = style.Bold(true).Foreground(lipgloss.Color("#a5b4fc"))
	}
	return style.Render(box + " " + label)
}

func (m *tuiModel) viewScanning() string {
	var b strings.Builder
	b.WriteString(titleStyle.Render("🗂 unused-removal"))
	b.WriteString("\n\n")

	var snap scanner.Snapshot
	if m.prog != nil {
		snap = m.prog.Snapshot()
	}

	// Сводка метрик
	metrics := lipgloss.JoinHorizontal(lipgloss.Top,
		metricBox("Файлы", fmt.Sprintf("%d", snap.Files)),
		metricBox("Каталоги", fmt.Sprintf("%d", snap.Dirs)),
		metricBox("Обработано", formatBytes(snap.Bytes)),
		metricBox("Скорость", fmt.Sprintf("%.0f ф/с", snap.RateFPS)),
	)

	var body strings.Builder
	body.WriteString(m.spinner.View() + " Сканирование: " +
		lipgloss.NewStyle().Foreground(lipgloss.Color("#d8dee9")).Render(m.cfg.Root))
	body.WriteString("\n\n")
	body.WriteString(metrics)
	body.WriteString("\n\n")
	body.WriteString(labelStyle.Render("Текущий каталог:"))
	body.WriteString("\n  " + infoStyle.Render(snap.Current))
	body.WriteString("\n\n")
	body.WriteString(labelStyle.Render("Последние файлы:"))
	body.WriteString("\n")

	// Последние обработанные файлы (свежие снизу)
	recent := snap.Recent
	show := recent
	if len(show) > 12 {
		show = show[len(show)-12:]
	}
	if len(show) == 0 {
		body.WriteString("  " + dimStyle.Render("…"))
	} else {
		for _, p := range show {
			body.WriteString("  " + dimStyle.Render(truncateMiddle(p, m.width-8)) + "\n")
		}
	}

	b.WriteString(panelStyle.Render(body.String()))

	if m.statusMsg != "" {
		b.WriteString("\n\n" + m.renderStatus())
	}
	b.WriteString("\n\n")
	b.WriteString(m.renderHelp("Сканирование"))
	return b.String()
}

// metricBox — компактный блок метрики для экрана сканирования.
func metricBox(label, value string) string {
	return lipgloss.NewStyle().
		Border(lipgloss.RoundedBorder()).
		BorderForeground(lipgloss.Color("#2e3440")).
		Padding(0, 1).
		Align(lipgloss.Center).
		Render(
			lipgloss.NewStyle().Foreground(lipgloss.Color("#8f9bb3")).Bold(true).Render(label) + "\n" +
				lipgloss.NewStyle().Foreground(lipgloss.Color("#e5e9f0")).Bold(true).Render(value),
		)
}

// truncateMiddle обрезает строку по центру (важно для длинных путей).
func truncateMiddle(s string, max int) string {
	if max <= 0 {
		max = 60
	}
	if len(s) <= max {
		return s
	}
	keep := max / 2
	return s[:keep] + "…" + s[len(s)-(max-keep-1):]
}

func (m *tuiModel) viewResults() string {
	var b strings.Builder
	b.WriteString(titleStyle.Render("🗂 unused-removal"))
	b.WriteString("\n\n")
	b.WriteString(panelStyle.Render(m.table.View()))
	b.WriteString("\n")
	b.WriteString(dimStyle.Render(fmt.Sprintf("Находок: %d · выбрано: %d", len(m.table.Rows()), len(m.selected))))
	if m.filterCat != "" {
		b.WriteString(" " + infoStyle.Render("· фильтр: "+categoryNameCLI(rules.Category(m.filterCat))))
	}
	if m.statusMsg != "" {
		b.WriteString("\n\n" + m.renderStatus())
	}
	b.WriteString("\n\n")
	b.WriteString(m.renderHelp("Результаты"))
	return b.String()
}

func (m *tuiModel) renderStatus() string {
	var style lipgloss.Style
	switch m.statusType {
	case "ok":
		style = okStyle
	case "err":
		style = errStyle
	default:
		style = infoStyle
	}
	return style.Render("• " + m.statusMsg)
}

func (m *tuiModel) renderHelp(section string) string {
	items := map[string]string{
		"Конфигурация": "Tab/↓ — навигация · Enter — сканировать · q — выход",
		"Сканирование": "s — остановить · q — выход",
		"Результаты":   "↑↓ — выбор · Пробел — отметить · t — в Корзину · x — безвозвратно · c — фильтр категории · r — назад · q — выход",
	}
	return dimStyle.Render(items[section])
}
