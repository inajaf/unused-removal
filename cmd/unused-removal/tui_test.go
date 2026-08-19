package main

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"unused-removal/internal/config"
	"unused-removal/internal/rules"
)

// keyPress создаёт KeyPressMsg из текста (печатный символ) или кода (спец-клавиша).
func keyPress(text string, code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg(tea.Key{Text: text, Code: code})
}

// ctrlC создаёт KeyPressMsg для Ctrl+C.
func ctrlC() tea.KeyPressMsg {
	return tea.KeyPressMsg(tea.Key{Code: 'c', Mod: tea.ModCtrl})
}

func TestTuiModel_InitialState(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)

	if m.state != stateConfig {
		t.Errorf("initial state = %v, want stateConfig", m.state)
	}
	if m.rootInput.Value() != cfg.Root {
		t.Errorf("root input = %q, want %q", m.rootInput.Value(), cfg.Root)
	}
	// Проверяем рендер конфига
	view := m.View().Content
	if !strings.Contains(view, "unused-removal") {
		t.Error("config view should contain app title")
	}
	if !strings.Contains(view, "Начать сканирование") {
		t.Error("config view should contain scan button")
	}
}

func TestTuiModel_ConfigNavigation(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)

	// Tab двигает фокус
	updated, _ := m.Update(keyPress("", tea.KeyTab))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.cfgFocus != 1 {
		t.Errorf("after Tab cfgFocus = %d, want 1", m2.cfgFocus)
	}
}

func TestTuiModel_ScanRequiresRoot(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.rootInput.SetValue("")

	// Enter на кнопке сканирования (фокус 4)
	m.cfgFocus = 4
	updated, _ := m.Update(keyPress("", tea.KeyEnter))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.state != stateConfig {
		t.Errorf("state = %v, want config (should reject empty root)", m2.state)
	}
	if m2.statusType != "err" {
		t.Errorf("statusType = %q, want err for empty root", m2.statusType)
	}
}

func TestTuiModel_StartScanTransitions(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.rootInput.SetValue(t.TempDir())
	m.cfgFocus = 4

	updated, _ := m.Update(keyPress("", tea.KeyEnter))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.state != stateScanning {
		t.Errorf("state = %v, want stateScanning after Enter", m2.state)
	}
	if m2.scanCh == nil {
		t.Error("scanCh should be created after start")
	}
}

func TestTuiModel_RebuildTable(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.findings = rulesFindingStub
	m.rebuildTable()

	if len(m.table.Rows()) != 2 {
		t.Errorf("rows = %d, want 2", len(m.table.Rows()))
	}
}

func TestTuiModel_FilterCategory(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.findings = rulesFindingStub
	m.rebuildTable()
	m.filterCat = "junk"
	m.rebuildTable()

	if len(m.table.Rows()) != 1 {
		t.Errorf("rows with junk filter = %d, want 1", len(m.table.Rows()))
	}
}

func TestTuiModel_SelectRow(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.findings = rulesFindingStub
	m.rebuildTable()
	m.state = stateResults
	m.table.SetCursor(0)

	updated, _ := m.Update(keyPress("", tea.KeySpace))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if len(m2.selected) != 1 {
		t.Errorf("selected count = %d, want 1", len(m2.selected))
	}
}

func TestTuiModel_BackToConfig(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.state = stateResults
	m.findings = rulesFindingStub
	m.rebuildTable()

	updated, _ := m.Update(keyPress("r", 'r'))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.state != stateConfig {
		t.Errorf("state = %v, want config after 'r'", m2.state)
	}
}

func TestTuiModel_DeleteNothingSelected(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.state = stateResults
	m.findings = rulesFindingStub
	m.rebuildTable()

	updated, _ := m.Update(keyPress("t", 't'))
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.statusType != "info" {
		t.Errorf("statusType = %q, want info when nothing selected", m2.statusType)
	}
}

func TestTuiModel_TableWidth(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)
	m.findings = rulesFindingStub
	m.rebuildTable()

	updated, _ := m.Update(tea.WindowSizeMsg{Width: 120, Height: 40})
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if m2.table.Width() <= 0 {
		t.Error("table width should be positive after resize")
	}
}

func TestTuiModel_Quit(t *testing.T) {
	cfg := config.DefaultConfig()
	m := newTuiModel(cfg)

	updated, _ := m.Update(ctrlC())
	m2, ok := updated.(*tuiModel)
	if !ok {
		t.Fatal("update returned non-tuiModel")
	}
	if !m2.quitting {
		t.Error("quitting should be true after ctrl+c")
	}
	view := m2.View().Content
	if !strings.Contains(view, "Спасибо") {
		t.Error("quit view should say thank you")
	}
}

// --- тестовые данные ---

var rulesFindingStub = []rules.Finding{
	{Path: `C:\Temp\junk1.tmp`, Size: 100, Category: rules.CatJunk, Risk: rules.RiskSafe, Reason: "tmp"},
	{Path: `C:\Data\big.bin`, Size: 5000, Category: rules.CatLarge, Risk: rules.RiskCaution, Reason: "big"},
}
