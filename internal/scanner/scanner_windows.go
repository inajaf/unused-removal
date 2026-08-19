//go:build windows
// +build windows

package scanner

import (
	"errors"
	"path/filepath"
	"strings"
	"time"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	fileAttributeDirectory = 0x00000010
	fileAttributeReparse   = 0x00000400
	fileAttributeHidden    = 0x00000002
	fileAttributeSystem    = 0x00000004

	fileFlagBackupSemantics = 0x02000000
	fileFlagOpenReparse     = 0x00200000
	fileShareRead           = 0x00000001
	fileShareWrite          = 0x00000002
	fileShareDelete         = 0x00000004
	genericRead             = 0x80000000
	openExisting            = 3

	// FindFirstFileExW параметры для пакетного чтения каталогов.
	// FindExInfoBasic (1) — не запрашивать короткие имена 8.3 (быстрее),
	// FIND_FIRST_EX_LARGE_FETCH (2) — ОС читает каталог большими пакетами,
	// а не по одной записи: 1 syscall на пачку записей вместо 1 на запись.
	findExInfoBasic       = 1
	findExSearchNameMatch = 0
	findFirstExLargeFetch = 0x00000002

	// 100ns интервалов в секунду
	filetimeTicksPerSec = 10_000_000
	// секунд от 1601-01-01 до 1970-01-01 (Unix epoch)
	filetimeUnixEpochOffsetSec = 11644473600
)

// FindFirstFileExW отсутствует в golang.org/x/sys/windows — объявляем сами.
// Пакетное перечисление записей каталога даёт значительный прирост скорости
// на каталогах с тысячами файлов (WinSxS, System32, node_modules и т.п.).
var (
	modKernel32          = windows.NewLazySystemDLL("kernel32.dll")
	procFindFirstFileExW = modKernel32.NewProc("FindFirstFileExW")
)

// winEntry — результат чтения одной записи каталога (внутренний формат).
type winEntry struct {
	Name      string
	Size      int64
	ModTime   time.Time
	IsDir     bool
	IsReparse bool
	Attr      Attrs
}

// entryAttrs — вспомогательная функция преобразования атрибутов Win32 в Attrs.
func entryAttrs(attrs uint32) Attrs {
	return Attrs{
		IsDir:     attrs&fileAttributeDirectory != 0,
		IsReparse: attrs&fileAttributeReparse != 0,
		IsHidden:  attrs&fileAttributeHidden != 0,
		IsSystem:  attrs&fileAttributeSystem != 0,
	}
}

// findFirstFileExW вызывает FindFirstFileExW с FindExInfoBasic + LARGE_FETCH.
// Возвращает handle и первую запись; при неудаче — ошибку.
func findFirstFileExW(pattern string, fd *windows.Win32finddata) (windows.Handle, error) {
	ptr, err := windows.UTF16PtrFromString(pattern)
	if err != nil {
		return 0, err
	}
	r1, _, e1 := procFindFirstFileExW.Call(
		uintptr(unsafe.Pointer(ptr)),
		uintptr(findExInfoBasic),
		uintptr(unsafe.Pointer(fd)),
		uintptr(findExSearchNameMatch),
		0, // lpSearchFilter — не используется
		uintptr(findFirstExLargeFetch),
	)
	if r1 == uintptr(windows.InvalidHandle) {
		if e1 != nil {
			return 0, e1
		}
		return 0, windows.ERROR_FILE_NOT_FOUND
	}
	return windows.Handle(r1), nil
}

// filetimeToTime конвертирует Win32 FILETIME (100ns с 1601-01-01) в time.Time.
func filetimeToTime(ft windows.Filetime) time.Time {
	ticks := (int64(ft.HighDateTime)<<32 + int64(ft.LowDateTime))
	sec := ticks/filetimeTicksPerSec - filetimeUnixEpochOffsetSec
	nsec := (ticks % filetimeTicksPerSec) * 100
	return time.Unix(sec, nsec).UTC()
}

// readDirEntries читает ВСЕ записи каталога dir через FindFirstFileExW/FindNextFileW.
// Возвращает отпечаток каталога (ModTime), список записей и ошибку (если доступ запрещён).
func readDirEntries(dir string) (Fingerprint, []winEntry, error) {
	// Перечисляем содержимое пакетами (FindFirstFileExW + LARGE_FETCH).
	// Отпечаток каталога берём из записи "." (первая запись) — это LastWriteTime
	// самого каталога, без отдельного вызова GetFileAttributesEx (минус 1 syscall
	// на каждый каталог — при сотнях тысяч каталогов это существенно).
	pattern := buildPattern(dir)
	var fd windows.Win32finddata
	h, err := findFirstFileExW(pattern, &fd)
	if err != nil {
		return Fingerprint{}, nil, err
	}
	defer windows.FindClose(h)

	fp := Fingerprint{ModTimeNS: filetimeToUnixNS(fd.LastWriteTime)}

	var entries []winEntry
	for {
		name := windows.UTF16ToString(fd.FileName[:])
		if name != "." && name != ".." {
			a := entryAttrs(fd.FileAttributes)
			e := winEntry{
				Name:      name,
				Size:      int64(fd.FileSizeHigh)<<32 | int64(fd.FileSizeLow),
				ModTime:   filetimeToTime(fd.LastWriteTime),
				IsDir:     a.IsDir,
				IsReparse: a.IsReparse,
				Attr:      a,
			}
			entries = append(entries, e)
		}
		err = windows.FindNextFile(h, &fd)
		if err != nil {
			if errors.Is(err, windows.ERROR_NO_MORE_FILES) {
				break
			}
			// Другие ошибки во время перечисления — запишем как ошибку каталога
			return fp, entries, err
		}
	}
	return fp, entries, nil
}

// filetimeToUnixNS возвращает Unix-наносекунды от Win32 FILETIME.
func filetimeToUnixNS(ft windows.Filetime) int64 {
	ticks := (int64(ft.HighDateTime)<<32 + int64(ft.LowDateTime))
	sec := ticks/filetimeTicksPerSec - filetimeUnixEpochOffsetSec
	return sec*1_000_000_000 + (ticks%filetimeTicksPerSec)*100
}

// buildPattern строит поисковый шаблон "dir\*" с поддержкой длинных путей.
func buildPattern(dir string) string {
	abs := filepath.Clean(dir)
	if !strings.HasSuffix(abs, "\\") {
		abs += "\\"
	}
	pattern := abs + "*"
	if len(pattern) > 248 && !strings.HasPrefix(pattern, `\\?\`) {
		pattern = `\\?\` + pattern
	}
	return pattern
}

// identityOf возвращает уникальный идентификатор каталога (volume serial + file index)
// для защиты от циклов при FollowLinks. Работает только для reparse-точек.
func identityOf(path string) (dirID, bool) {
	ptr, err := windows.UTF16PtrFromString(path)
	if err != nil {
		return dirID{}, false
	}
	h, err := windows.CreateFile(
		ptr,
		genericRead,
		fileShareRead|fileShareWrite|fileShareDelete,
		nil,
		openExisting,
		fileFlagBackupSemantics|fileFlagOpenReparse,
		0,
	)
	if err != nil {
		return dirID{}, false
	}
	defer windows.CloseHandle(h)

	var info windows.ByHandleFileInformation
	err = windows.GetFileInformationByHandle(h, &info)
	if err != nil {
		return dirID{}, false
	}
	return dirID{
		vol: info.VolumeSerialNumber,
		idx: (uint64(info.FileIndexHigh) << 32) | uint64(info.FileIndexLow),
	}, true
}
