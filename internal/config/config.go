package config

import (
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/BurntSushi/toml"
	"unused-removal/internal/scanner"
)

// Config — полная конфигурация приложения.
type Config struct {
	// Общие
	Root          string   `toml:"root" json:"root"`
	Workers       int      `toml:"workers" json:"workers"`
	FollowLinks   bool     `toml:"follow_links" json:"follow_links"`
	ExcludeDirs   []string `toml:"exclude_dirs" json:"exclude_dirs"`
	ExcludePrefix []string `toml:"exclude_prefix" json:"exclude_prefix"`

	// Правила поиска
	LargeBytes       int64    `toml:"large_bytes" json:"large_bytes"`               // порог "крупный файл"
	HugeBytes        int64    `toml:"huge_bytes" json:"huge_bytes"`                 // порог "очень крупный файл"
	StaleDays        int      `toml:"stale_days" json:"stale_days"`                 // дней без изменений → "старые"
	OldLogDays       int      `toml:"old_log_days" json:"old_log_days"`             // дней без изменений → старые логи (junk)
	StaleInstallDays int      `toml:"stale_install_days" json:"stale_install_days"` // дней без изменений → старые инсталляторы
	JunkExtensions   []string `toml:"junk_extensions" json:"junk_extensions"`       // расширения-мусор
	JunkDirs         []string `toml:"junk_dirs" json:"junk_dirs"`                   // известные мусорные каталоги
	CheckDuplicates  bool     `toml:"check_duplicates" json:"check_duplicates"`     // искать дубликаты

	// Безопасность
	ProtectSystem  bool `toml:"protect_system" json:"protect_system"`   // защита системных путей
	AllowProtected bool `toml:"allow_protected" json:"allow_protected"` // разрешить предлагать защищённые

	// Кэш
	UseCache bool   `toml:"use_cache" json:"use_cache"`
	CacheDir string `toml:"cache_dir" json:"cache_dir"`

	// Веб-интерфейс
	WebPort int `toml:"web_port" json:"web_port"`
}

// DefaultConfig возвращает конфиг с разумными дефолтами для Windows.
func DefaultConfig() *Config {
	return &Config{
		Root:             `C:\`,
		Workers:          0, // auto = NumCPU
		FollowLinks:      false,
		ExcludeDirs:      []string{`$Recycle.Bin`, `System Volume Information`, `Windows\WinSxS`, `Windows\SoftwareDistribution`, `ProgramData\Microsoft\Windows Defender`},
		ExcludePrefix:    []string{},
		LargeBytes:       100 * 1024 * 1024, // 100 МБ
		HugeBytes:        500 * 1024 * 1024, // 500 МБ
		StaleDays:        180,
		OldLogDays:       30,
		StaleInstallDays: 90,
		JunkExtensions:   []string{`.tmp`, `.temp`, `.bak`, `.old`, `.dmp`, `.chk`, `~$*`},
		JunkDirs:         []string{`%TEMP%`, `C:\Windows\Temp`, `C:\Windows\Prefetch`, `%LOCALAPPDATA%\Google\Chrome\User Data\Default\Cache`, `%LOCALAPPDATA%\Microsoft\Edge\User Data\Default\Cache`, `%LOCALAPPDATA%\Mozilla\Firefox\Profiles`},
		CheckDuplicates:  false,
		ProtectSystem:    true,
		AllowProtected:   false,
		UseCache:         true,
		CacheDir:         ``,
		WebPort:          0, // auto
	}
}

// expandEnvVars разворачивает переменные окружения Windows-стиля (%VAR%).
// Важно: $VAR НЕ разворачивается — в Windows $ это обычный символ имени папки
// (например "$Recycle.Bin"), а не префикс переменной. Раньше os.Expand ломал
// "$Recycle.Bin", накапливая ".Bin" при каждом сохранении конфига.
func expandEnvVars(paths []string) []string {
	res := make([]string, 0, len(paths))
	for _, p := range paths {
		res = append(res, expandPercentVars(p))
	}
	return res
}

// expandPercentVars раскрывает только %VAR% (Windows-стиль), не трогая $ и ${}.
func expandPercentVars(s string) string {
	var b strings.Builder
	b.Grow(len(s))
	for i := 0; i < len(s); {
		if s[i] == '%' {
			j := strings.IndexByte(s[i+1:], '%')
			if j >= 0 {
				name := s[i+1 : i+1+j]
				if val, ok := os.LookupEnv(name); ok {
					b.WriteString(val)
				} else {
					// Не найдена — оставляем %VAR% как есть.
					b.WriteString(s[i : i+1+j+1])
				}
				i += j + 2
				continue
			}
		}
		b.WriteByte(s[i])
		i++
	}
	return b.String()
}

// Load читает config.toml из стандартных мест и мержит с дефолтами.
// Порядок приоритета: 1) явный путь, 2) ./config.toml, 3) %LOCALAPPDATA%\unused-removal\config.toml.
func Load(explicitPath string) (*Config, error) {
	cfg := DefaultConfig()

	paths := []string{}
	if explicitPath != "" {
		paths = append(paths, explicitPath)
	}
	paths = append(paths, "config.toml")
	if local := os.Getenv("LOCALAPPDATA"); local != "" {
		paths = append(paths, filepath.Join(local, "unused-removal", "config.toml"))
	}

	for _, p := range paths {
		if _, err := os.Stat(p); err == nil {
			meta, err := toml.DecodeFile(p, cfg)
			if err != nil {
				return nil, err
			}
			if meta.Undecoded() != nil {
				// можно логировать предупреждение
			}
			break
		}
	}

	// Раскрываем переменные окружения в путях
	cfg.ExcludeDirs = expandEnvVars(cfg.ExcludeDirs)
	cfg.ExcludePrefix = expandEnvVars(cfg.ExcludePrefix)
	cfg.JunkDirs = expandEnvVars(cfg.JunkDirs)

	// Валидация и дефолты
	if cfg.Workers <= 0 {
		cfg.Workers = 0 // Walker сам поставит NumCPU
	}
	if cfg.LargeBytes <= 0 {
		cfg.LargeBytes = 100 * 1024 * 1024
	}
	if cfg.HugeBytes <= 0 {
		cfg.HugeBytes = 500 * 1024 * 1024
	}
	if cfg.StaleDays <= 0 {
		cfg.StaleDays = 180
	}
	if cfg.OldLogDays <= 0 {
		cfg.OldLogDays = 30
	}
	if cfg.StaleInstallDays <= 0 {
		cfg.StaleInstallDays = 90
	}
	if cfg.WebPort <= 0 {
		cfg.WebPort = 0 // auto
	}

	return cfg, nil
}

// Save сохраняет конфиг в файл (используется UI для настроек).
func (c *Config) Save(path string) error {
	f, err := os.Create(path)
	if err != nil {
		return err
	}
	defer f.Close()
	return toml.NewEncoder(f).Encode(c)
}

// ScannerOptions преобразует Config в scanner.Options.
func (c *Config) ScannerOptions() *scanner.Options {
	return &scanner.Options{
		Workers:     c.Workers,
		FollowLinks: c.FollowLinks,
		Exclude:     c.ExcludeDirs,
		ExcludePref: c.ExcludePrefix,
	}
}

// Time thresholds для правил.
func (c *Config) StaleCutoff() time.Time {
	return time.Now().AddDate(0, 0, -c.StaleDays)
}
func (c *Config) OldLogCutoff() time.Time {
	return time.Now().AddDate(0, 0, -c.OldLogDays)
}
func (c *Config) StaleInstallCutoff() time.Time {
	return time.Now().AddDate(0, 0, -c.StaleInstallDays)
}
