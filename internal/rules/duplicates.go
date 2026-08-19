package rules

import (
	"encoding/hex"
	"io"
	"os"

	"github.com/zeebo/blake3"
)

// hashFile вычисляет blake3 хэш файла (быстрый, параллельный внутри).
func hashFile(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", err
	}
	defer f.Close()

	h := blake3.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", err
	}
	return hex.EncodeToString(h.Sum(nil)), nil
}
