package cleaner

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"unsafe"

	"golang.org/x/sys/windows"
)

const (
	// SHFileOperation flags
	foDelete     = 3
	fofAllowUndo = 0x0040 // в Корзину
	fofNoConfirm = 0x0010 // без диалога подтверждения
	fofNoErrorUI = 0x0400 // без UI ошибок
	fofSilent    = 0x0004 // без прогресс-диалога
	fofWantNuke  = 0x0001 // без Корзины (жёсткое удаление)
)

// SHFILEOPSTRUCTW — структура для SHFileOperationW.
type shfileopstructW struct {
	hwnd                  uintptr
	wFunc                 uint32
	pFrom                 *uint16
	pTo                   *uint16
	fFlags                uint16
	fAnyOperationsAborted int32
	hNameMappings         uintptr
	lpszProgressTitle     *uint16
}

var (
	shell32             = windows.NewLazySystemDLL("shell32.dll")
	procSHFileOperation = shell32.NewProc("SHFileOperationW")
)

// Result — результат удаления.
type Result struct {
	Deleted    []string      `json:"deleted"` // успешно удалённые пути
	Failed     []DeleteError `json:"failed"`  // ошибки
	TotalBytes int64         `json:"total_bytes"`
}

// DeleteError — ошибка удаления одного пути.
type DeleteError struct {
	Path string `json:"path"`
	Err  string `json:"error"`
}

func (e DeleteError) Error() string { return e.Path + ": " + e.Err }

// RecycleBin перемещает файлы/каталоги в Корзину.
// Возвращает агрегированный результат.
func RecycleBin(paths []string) (Result, error) {
	if len(paths) == 0 {
		return Result{}, nil
	}
	return doSHFileOp(paths, fofAllowUndo|fofNoConfirm|fofNoErrorUI|fofSilent)
}

// HardDelete удаляет файлы/каталоги безвозвратно (обходит Корзину).
func HardDelete(paths []string) (Result, error) {
	if len(paths) == 0 {
		return Result{}, nil
	}
	// Сначала пробуем SHFileOperation с FOF_WANTNUKE
	res, err := doSHFileOp(paths, fofWantNuke|fofNoConfirm|fofNoErrorUI|fofSilent)
	if err == nil {
		return res, nil
	}
	// Фоллбэк: os.RemoveAll поштучно
	return hardDeleteFallback(paths)
}

// doSHFileOp вызывает SHFileOperationW.
// paths — список путей (должны быть абсолютными).
// flags — комбинация FOF_*.
func doSHFileOp(paths []string, flags uint16) (Result, error) {
	// Построим двойной-нуль-терминированный список путей (UTF-16)
	var from strings.Builder
	var totalBytes int64
	for _, p := range paths {
		// нормализуем путь
		abs, err := filepathAbs(p)
		if err != nil {
			continue
		}
		// Проверяем существование (если нет — не ошибка, просто пропускаем)
		if _, err := osStat(abs); err != nil {
			continue
		}
		// Получаем размер для статистики (файлы; каталоги — рекурсивно не считаем)
		if fi, err := osStat(abs); err == nil && !fi.IsDir() {
			totalBytes += fi.Size()
		}
		from.WriteString(abs)
		from.WriteByte(0)
	}
	from.WriteByte(0) // двойной ноль

	fromPtr, err := windows.UTF16PtrFromString(from.String())
	if err != nil {
		return Result{}, err
	}

	op := shfileopstructW{
		hwnd:                  0,
		wFunc:                 foDelete,
		pFrom:                 fromPtr,
		pTo:                   nil,
		fFlags:                flags,
		fAnyOperationsAborted: 0,
		hNameMappings:         0,
		lpszProgressTitle:     nil,
	}

	ret, _, _ := procSHFileOperation.Call(uintptr(unsafe.Pointer(&op)))
	// ret == 0 — успех, иначе — код ошибки
	if ret != 0 {
		// SHFileOperation не даёт детальных ошибок по файлам; вернём общую
		return Result{}, fmt.Errorf("SHFileOperation failed with code %d", ret)
	}
	return Result{Deleted: paths, TotalBytes: totalBytes}, nil
}

// hardDeleteFallback — поштучное удаление через os.RemoveAll с сбором ошибок.
func hardDeleteFallback(paths []string) (Result, error) {
	var res Result
	var mu sync.Mutex
	var wg sync.WaitGroup
	errs := make([]DeleteError, 0, len(paths))
	deleted := make([]string, 0, len(paths))

	for _, p := range paths {
		wg.Add(1)
		go func(path string) {
			defer wg.Done()
			abs, err := filepathAbs(path)
			if err != nil {
				mu.Lock()
				errs = append(errs, DeleteError{Path: path, Err: err.Error()})
				mu.Unlock()
				return
			}
			if fi, err := osStat(abs); err == nil && !fi.IsDir() {
				mu.Lock()
				res.TotalBytes += fi.Size()
				mu.Unlock()
			}
			err = osRemoveAll(abs)
			mu.Lock()
			if err != nil {
				errs = append(errs, DeleteError{Path: path, Err: err.Error()})
			} else {
				deleted = append(deleted, path)
			}
			mu.Unlock()
		}(p)
	}
	wg.Wait()
	res.Deleted = deleted
	res.Failed = errs
	if len(errs) > 0 {
		return res, errors.New("some files failed to delete")
	}
	return res, nil
}

// --- тонкие обёртки над os для тестируемости/мока ---

func filepathAbs(path string) (string, error) {
	return filepath.Abs(path)
}

func osStat(path string) (os.FileInfo, error) {
	return os.Stat(path)
}

func osRemoveAll(path string) error {
	return os.RemoveAll(path)
}

// Нужен import filepath, os
