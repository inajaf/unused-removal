//go:build windows
// +build windows

package scanner

import (
	"bytes"
	"encoding/binary"
	"encoding/gob"
	"os"
	"path/filepath"
	"strings"
	"time"

	"go.etcd.io/bbolt"
)

const (
	cacheBucketFP   = "fp"   // Fingerprint + записи каталога
	cacheBucketMeta = "meta" // метаданные скана (версия, config hash)
)

// BoltCache — реализация Cache на bbolt.
type BoltCache struct {
	db   *bbolt.DB
	path string
	gen  string // generation/config hash для инвалидации при смене настроек
}

func init() {
	// Регистрируем типы для gob (нужно для кодирования CacheEntry)
	gob.Register(CacheEntry{})
	gob.Register(Fingerprint{})
	gob.Register(Attrs{})
	gob.Register(FileRecord{})
}

// NewBoltCache открывает/создаёт кэш в стандартной папке приложения.
func NewBoltCache(appName string, configHash string) (*BoltCache, error) {
	dir, err := os.UserCacheDir()
	if err != nil {
		return nil, err
	}
	cacheDir := filepath.Join(dir, appName)
	if err := os.MkdirAll(cacheDir, 0o700); err != nil {
		return nil, err
	}
	dbPath := filepath.Join(cacheDir, "scan_cache.bbolt")
	// Timeout обязателен: если другой процесс (веб + TUI одновременно) уже держит
	// файл кэша, bbolt.Open с nil опциями блокируется НАВСЕГДА. С таймаутом второй
	// процесс просто отключит кэш вместо зависания.
	db, err := bbolt.Open(dbPath, 0o600, &bbolt.Options{Timeout: 2 * time.Second})
	if err != nil {
		return nil, err
	}
	// Создаём бакеты и проверяем поколение кэша: если конфигурация (config hash)
	// изменилась — кэш устарел, чистим его целиком.
	err = db.Update(func(tx *bbolt.Tx) error {
		meta, err := tx.CreateBucketIfNotExists([]byte(cacheBucketMeta))
		if err != nil {
			return err
		}
		stored := string(meta.Get([]byte("gen")))
		if stored == configHash {
			return nil
		}
		// Поколение не совпало — инвалидируем все отпечатки.
		if b := tx.Bucket([]byte(cacheBucketFP)); b != nil {
			if err := tx.DeleteBucket([]byte(cacheBucketFP)); err != nil {
				return err
			}
		}
		if _, err := tx.CreateBucketIfNotExists([]byte(cacheBucketFP)); err != nil {
			return err
		}
		return meta.Put([]byte("gen"), []byte(configHash))
	})
	if err != nil {
		_ = db.Close()
		return nil, err
	}
	return &BoltCache{db: db, path: dbPath, gen: configHash}, nil
}

// Lookup ищет запись каталога; возвращает false, если нет записи, поколение
// не совпадает или отпечаток изменился.
func (c *BoltCache) Lookup(dir string, fp Fingerprint) (CacheEntry, bool) {
	var ent CacheEntry
	err := c.db.View(func(tx *bbolt.Tx) error {
		b := tx.Bucket([]byte(cacheBucketFP))
		if b == nil {
			return nil
		}
		data := b.Get([]byte(dir))
		if data == nil {
			return nil
		}
		return decodeEntry(data, &ent)
	})
	if err != nil || ent.Files == nil {
		return CacheEntry{}, false
	}
	if c.gen != "" && ent.FP.ModTimeNS != fp.ModTimeNS {
		// отпечаток изменился — невалидно
		return CacheEntry{}, false
	}
	return ent, true
}

// SaveTotal сохраняет общее число файлов прошлого скана (для оценки прогресса).
func (c *BoltCache) SaveTotal(n int64) error {
	return c.db.Update(func(tx *bbolt.Tx) error {
		b := tx.Bucket([]byte(cacheBucketMeta))
		if b == nil {
			return nil
		}
		var buf [8]byte
		binary.LittleEndian.PutUint64(buf[:], uint64(n))
		return b.Put([]byte("total"), buf[:])
	})
}

// LoadTotal возвращает сохранённое общее число файлов прошлого скана (0 если нет).
func (c *BoltCache) LoadTotal() int64 {
	var n int64
	_ = c.db.View(func(tx *bbolt.Tx) error {
		b := tx.Bucket([]byte(cacheBucketMeta))
		if b == nil {
			return nil
		}
		data := b.Get([]byte("total"))
		if len(data) == 8 {
			n = int64(binary.LittleEndian.Uint64(data))
		}
		return nil
	})
	return n
}

// Save сохраняет запись каталога.
// db.Batch коалесцирует конкурентные транзакции записи в одну — при массовом
// кэшировании каталогов это на порядок быстрее, чем отдельный db.Update на каждый.
func (c *BoltCache) Save(dir string, e CacheEntry) error {
	return c.db.Batch(func(tx *bbolt.Tx) error {
		b := tx.Bucket([]byte(cacheBucketFP))
		if b == nil {
			return nil
		}
		data, err := encodeEntry(e)
		if err != nil {
			return err
		}
		return b.Put([]byte(dir), data)
	})
}

func (c *BoltCache) Close() error { return c.db.Close() }

// encodeEntry / decodeEntry — сериализация через gob.
func encodeEntry(e CacheEntry) ([]byte, error) {
	var buf bytes.Buffer
	enc := gob.NewEncoder(&buf)
	if err := enc.Encode(e); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

func decodeEntry(data []byte, e *CacheEntry) error {
	dec := gob.NewDecoder(bytes.NewReader(data))
	return dec.Decode(e)
}

// configHash вычисляет простой хеш конфигурации для инвалидации кэша при смене настроек.
// В проде можно использовать crypto/sha256, здесь — FNV.
func configHash(opts Options) string {
	var sb strings.Builder
	sb.WriteString("w:")
	sb.WriteString(itoa64(uint64(opts.Workers)))
	sb.WriteString(" fl:")
	sb.WriteString(itoa64(uint64(boolToInt(opts.FollowLinks))))
	for _, e := range opts.Exclude {
		sb.WriteString(" x:")
		sb.WriteString(e)
	}
	for _, p := range opts.ExcludePref {
		sb.WriteString(" xp:")
		sb.WriteString(p)
	}
	// FNV-1a 64-bit
	var h uint64 = 1469598103934665603
	for i := 0; i < sb.Len(); i++ {
		h ^= uint64(sb.String()[i])
		h *= 1099511628211
	}
	return itoa64(h)
}

func itoa64(i uint64) string {
	if i == 0 {
		return "0"
	}
	var b [32]byte
	n := len(b)
	for i > 0 {
		n--
		b[n] = byte('0' + i%10)
		i /= 10
	}
	return string(b[n:])
}
func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}
