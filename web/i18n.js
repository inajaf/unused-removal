const STORAGE_KEY = 'unused-removal-language';

let currentLanguage = localStorage.getItem(STORAGE_KEY) === 'ru' ? 'ru' : 'en';
let applying = false;
const originalText = new WeakMap();
const originalAttributes = new WeakMap();

const RU_TO_EN = {
  'unused-removal — Поиск и удаление ненужных файлов': 'unused-removal — Find and remove unused files',
  'Перейти к основному содержанию': 'Skip to main content',
  'Быстрый поиск и удаление ненужных файлов': 'Find space. Understand it. Reclaim it safely.',
  'Расширенные настройки сканирования': 'Advanced scan settings',
  'Расширенные настройки': 'Advanced settings',
  'Язык интерфейса': 'Interface language',
  'Умная очистка': 'Smart cleanup',
  'Автоматический поиск и удаление мусора одной кнопкой — как в CleanMyMac': 'One guided scan for large files, junk, leftovers, and duplicates',
  'Освободите место за секунды': 'See what is taking your space',
  'Сканирует кэши браузеров, логи, временные файлы, старые загрузки, кэши разработчика и корзину': 'Scans every selected drive and explains what is large, stale, duplicated, or safe to clean',
  'Параллельное сканирование': 'Parallel file scanning',
  'Системные пути защищены': 'System paths stay protected',
  'Удаление в Корзину': 'Recycle Bin by default',
  'Инкрементальный кэш': 'Incremental scan cache',
  'Где сканировать': 'Where to scan',
  'Папка для сканирования': 'Scan location',
  'Например: D:\\Projects или C:\\Users\\Name\\Downloads': 'Example: D:\\Projects or C:\\Users\\Name\\Downloads',
  'Применить': 'Apply',
  'Выберите уровень защиты': 'Choose a protection level',
  '— сканирование начнётся сразу': '— scanning starts immediately',
  'Безопасный': 'Safe',
  'Сбалансированный': 'Balanced',
  'Агрессивный': 'Aggressive',
  'Уровень безопасности': 'Safety level',
  'Только очевидный мусор — максимальная осторожность': 'Only obvious junk — maximum caution',
  'Кэши, логи, корзина и старые загрузки — только очевидный мусор': 'Caches, logs, Recycle Bin, and old downloads — obvious junk only',
  'Запустить': 'Start',
  'Оптимальный баланс освобождения и безопасности': 'The recommended balance of reclaimed space and safety',
  'Рекомендуем': 'Recommended',
  'Дополнительно локализации, бэкапы и вложения почты': 'Also reviews localizations, backups, and mail attachments',
  'Максимальное освобождение места': 'Maximum space recovery',
  'Всё выше + образы дисков, скрытые файлы и дубликаты': 'Everything above, plus disk images, hidden files, and duplicates',
  'Ничего не удаляется без вашего подтверждения — файлы отправляются в Корзину и могут быть восстановлены': 'Nothing is removed without your confirmation — files go to the Recycle Bin and can be restored',
  'папок': 'folders',
  'файлов': 'files',
  'недоступно': 'unavailable',
  'Часть папок закрыта правами macOS (Full Disk Access)': 'Some folders require macOS Full Disk Access',
  'Подготовка…': 'Preparing…',
  'Остановить сканирование': 'Stop scanning',
  'Настройка сканирования': 'Scan settings',
  'Выберите диск или папку и настройте параметры поиска': 'Choose a drive or folder and configure the scan',
  'Корневой путь': 'Root path',
  'Например: D:\\Projects или \\\\server\\share': 'Example: D:\\Projects or \\\\server\\share',
  'Потоки сканирования': 'Scanner threads',
  '0 = авто (все ядра CPU)': '0 = automatic (all CPU cores)',
  'Следовать за junction / symlink': 'Follow junctions / symlinks',
  'Может вызвать зацикливание, используйте с осторожностью': 'May cause loops; use with caution',
  'Повторные сканы будут в разы быстрее': 'Repeat scans are significantly faster',
  'Поиск дубликатов': 'Find duplicates',
  'Медленно: хэширует файлы одинакового размера': 'Slower: hashes files with matching sizes',
  'Защита системных путей': 'Protect system paths',
  'Windows, Program Files и критичные файлы не будут предложены к удалению': 'Windows, Program Files, and critical files will never be offered for deletion',
  'Назад к умной очистке': 'Back to smart cleanup',
  'Начать сканирование': 'Start scan',
  'Остановить': 'Stop',
  'Сканирование…': 'Scanning…',
  'Обход файловой системы…': 'Walking the file system…',
  'каталогов': 'directories',
  'обработано': 'processed',
  'файлов/с': 'files/sec',
  'время / осталось': 'elapsed / remaining',
  'из кэша': 'from cache',
  'Готовность…': 'Getting ready…',
  'Последние обработанные файлы': 'Recently processed files',
  'Результаты умной очистки': 'Smart cleanup results',
  'Найдено': 'Found',
  'файлов, можно освободить': 'files, reclaimable',
  'можно освободить': 'reclaimable',
  '· Папка:': '· Location:',
  'Папка:': 'Location:',
  'Уровень задаётся карточкой на главном экране перед запуском сканирования': 'Choose the level on the home screen before scanning',
  'Уровень сканирования:': 'Scan level:',
  'Очистить выбранное': 'Clean selected',
  'Просмотреть детально': 'Review details',
  'Занято': 'Used',
  'Мусор': 'Cleanable',
  'Категории мусора': 'Cleanup categories',
  'Результаты': 'Results',
  'находок (': 'findings (',
  'находок': 'findings',
  'Новое сканирование': 'New scan',
  'Категория': 'Category',
  'Все категории': 'All categories',
  'Очень крупные': 'Huge files',
  'Крупные': 'Large files',
  'Старые логи': 'Old logs',
  'Старые инсталляторы': 'Old installers',
  'Не использовались давно': 'Stale files',
  'Дубликаты': 'Duplicates',
  'Следы приложений': 'App leftovers',
  'Кэш пользователя': 'User cache',
  'Системные логи': 'System logs',
  'Кэш разработчика': 'Developer cache',
  'Xcode кэш': 'Xcode cache',
  'VS Code кэш': 'VS Code cache',
  'Корзина': 'Recycle Bin',
  'Старые загрузки': 'Old downloads',
  'Файлы локализации': 'Localization files',
  'Старые бэкапы': 'Old backups',
  'Вложения почты': 'Mail attachments',
  'Образы дисков': 'Disk images',
  'Скрытые файлы': 'Hidden files',
  'Поиск': 'Search',
  'Поиск по пути, имени или причине…': 'Search by path, name, or reason…',
  'Очистить поиск': 'Clear search',
  'выбрано,': 'selected,',
  'Выбрать все видимые': 'Select all visible',
  'Файл': 'File',
  'Размер': 'Size',
  'Причина': 'Reason',
  'Риск': 'Risk',
  'Изменён': 'Modified',
  'Предыдущая страница': 'Previous page',
  'Следующая страница': 'Next page',
  'Стр. 1 / 1': 'Page 1 / 1',
  'В Корзину': 'To Recycle Bin',
  'Безвозвратно': 'Delete permanently',
  'Экспорт JSON': 'Export JSON',
  'Экспорт CSV': 'Export CSV',
  'unused-removal — одиночный EXE, без зависимостей, open source': 'unused-removal — a focused desktop app, open source',
  'Используйте на свой страх и риск. Всегда проверяйте список перед удалением.': 'Always review the list before deleting files.',
  'Подтверждение': 'Confirmation',
  'Закрыть': 'Close',
  'Отмена': 'Cancel',
  'Подтвердить': 'Confirm',

  'Очень крупные файлы': 'Huge files',
  'Крупные файлы': 'Large files',
  'Временные и мусорные файлы': 'Temporary and junk files',
  'Старые лог-файлы': 'Old log files',
  'Не используемые давно': 'Stale files',
  'Следы удалённых приложений': 'Uninstalled app leftovers',
  'Кэш браузеров и приложений': 'Browser and application caches',
  'Системные и приложения логи': 'System and application logs',
  'Неиспользуемые локализации': 'Unused localizations',
  'Старые бэкапы (iOS, Time Machine, Windows)': 'Old backups (iOS, Time Machine, Windows)',
  'Старые вложения почты': 'Old mail attachments',
  'Корзина / Recycle Bin': 'Trash / Recycle Bin',
  'Старые файлы в Загрузках': 'Old files in Downloads',
  'Неиспользуемые образы дисков': 'Unused disk images',
  'Кэш инструментов разработки (npm, cargo, pip, gradle)': 'Developer tool caches (npm, cargo, pip, gradle)',
  'VS Code / Cursor кэш и логи': 'VS Code / Cursor caches and logs',
  'Большие скрытые файлы': 'Large hidden files',
  'Кэш приложений': 'Application caches',
  'Локализация': 'Localization files',
  'Безопасно': 'Safe',
  'Осторожно': 'Review',
  'Защищено': 'Protected',
  'Только кэши, логи, корзина, старые загрузки — максимальная безопасность': 'Caches, logs, Recycle Bin, and old downloads — maximum safety',
  '+ языки, бэкапы, вложения почты — рекомендуемый баланс': '+ localizations, backups, and mail attachments — recommended balance',
  'Всё включённое + образы дисков, скрытые файлы, дубликаты — максимальное освобождение': 'Everything above, plus disk images, hidden files, and duplicates — maximum recovery',
  'Указать свой путь…': 'Choose a custom path…',
  'Весь диск': 'Entire disk',
  'Текущая папка': 'Current folder',
  'Указать произвольную папку': 'Choose a custom folder',
  'Своя папка…': 'Custom folder…',
  'Ещё нет файлов…': 'No files yet…',
  'Завершено': 'Complete',
  'Индексация файловой системы…': 'Indexing the file system…',
  'Сканирование файлов…': 'Scanning files…',
  'Классификация найденных файлов…': 'Classifying discovered files…',
  'Подготовка результатов…': 'Preparing results…',
  'Проверка содержимого дубликатов…': 'Checking duplicate content…',
  'Сортировка результатов…': 'Sorting results…',
  'Сортировка и проверка безопасности…': 'Sorting and applying safety checks…',
  'Подготовка Windows-сканирования…': 'Preparing Windows scan…',
  'Здесь нечего чистить': 'Nothing to clean here',
  'Сменить папку сканирования': 'Choose another scan location',
  'Выбрать': 'Select',
  'Только просмотр': 'View only',
  'Развернуть': 'Expand',
  'Очистка в Корзину': 'Move selected items to Recycle Bin',
  'Удалить навсегда': 'Delete permanently',
  'Защищённый системный путь — только просмотр': 'Protected system path — view only',
  'пусто': 'empty',
  'Безвозвратное удаление': 'Permanent deletion',
  'Перемещение в Корзину': 'Move to Recycle Bin'
};

const ATTRIBUTES = ['title', 'placeholder', 'aria-label', 'data-tooltip'];

export function getLanguage() {
  return currentLanguage;
}

export function isEnglish() {
  return currentLanguage === 'en';
}

export function tr(value) {
  if (value == null || currentLanguage === 'ru') return value;
  const text = String(value);
  const exact = RU_TO_EN[text];
  if (exact) return exact;

  const trimmed = text.trim();
  const translated = RU_TO_EN[trimmed] || translatePattern(trimmed);
  if (translated === trimmed) return text;
  const start = text.slice(0, text.indexOf(trimmed));
  const end = text.slice(text.indexOf(trimmed) + trimmed.length);
  return start + translated + end;
}

function translatePattern(text) {
  const patterns = [
    [/^Диск ([A-Z]):$/i, 'Drive $1:'],
    [/^Сканирование (.+)$/, 'Scanning $1'],
    [/^Цель сканирования: (.+)$/, 'Scan target: $1'],
    [/^Ошибка запуска: (.+)$/, 'Could not start: $1'],
    [/^Ошибка остановки: (.+)$/, 'Could not stop: $1'],
    [/^Ошибка загрузки результатов: (.+)$/, 'Could not load results: $1'],
    [/^Ошибка очистки: (.+)$/, 'Cleanup error: $1'],
    [/^Ошибка удаления: (.+)$/, 'Deletion error: $1'],
    [/^Готово: удалено (.+), ошибок (.+)$/, 'Done: $1 deleted, $2 errors'],
    [/^Отчёт (.+) скачан$/, '$1 report exported'],
    [/^Стр\. (\d+) \/ (\d+)$/, 'Page $1 / $2'],
    [/^очень крупный файл \(> (.+)\)$/, 'huge file (> $1)'],
    [/^крупный файл \(> (.+)\)$/, 'large file (> $1)'],
    [/^большой скрытый файл \(> (.+)\)$/, 'large hidden file (> $1)'],
    [/^старый лог \(> (\d+) дней\)$/, 'old log (> $1 days)'],
    [/^старый системный лог \(> (\d+) дней\)$/, 'old system log (> $1 days)'],
    [/^старый инсталлятор в Downloads \(> (\d+) дней\)$/, 'old installer in Downloads (> $1 days)'],
    [/^старый файл в Downloads \(> (\d+) дней\)$/, 'old file in Downloads (> $1 days)'],
    [/^не менялся > (\d+) дней$/, 'unchanged for more than $1 days'],
    [/^расширение (.+)$/, 'file extension $1'],
    [/^в мусорном каталоге: (.+)$/, 'inside junk folder: $1'],
    [/^следы удалённого приложения \((.+)\)$/, 'uninstalled app leftovers ($1)'],
    [/^браузерный кэш \((.+)\)$/, 'browser cache ($1)'],
    [/^кэш разработчика: (.+)$/, 'developer cache: $1'],
    [/^Xcode кэш: (.+)$/, 'Xcode cache: $1'],
    [/^VS Code\/Cursor кэш: (.+)$/, 'VS Code/Cursor cache: $1'],
    [/^JetBrains IDE кэш: (.+)$/, 'JetBrains IDE cache: $1'],
    [/^неиспользуемый образ диска \((.+)\)$/, 'unused disk image ($1)'],
    [/^дубликат файла (.+)$/, 'duplicate of $1']
  ];
  for (const [pattern, replacement] of patterns) {
    if (pattern.test(text)) return text.replace(pattern, replacement);
  }

  const phrases = [
    ['; защищённый системный путь — только просмотр', '; protected system path — view only'],
    ['временный файл Office (~$*)', 'temporary Office file (~$*)'],
    ['возможные следы удалённого приложения', 'possible uninstalled app leftovers'],
    ['системный/пользовательский кэш macOS', 'macOS system/user cache'],
    ['системный/пользовательский кэш Windows', 'Windows system/user cache'],
    ['кэш приложения', 'application cache'],
    ['системный лог macOS', 'macOS system log'],
    ['системный лог Windows', 'Windows system log'],
    ['корзина macOS', 'macOS Trash'],
    ['корзина Windows', 'Windows Recycle Bin'],
    ['корзина Linux', 'Linux Trash'],
    ['вложение Apple Mail', 'Apple Mail attachment'],
    ['вложение почты Windows/Outlook', 'Windows/Outlook mail attachment'],
    ['старая резервная копия iOS (Windows)', 'old iOS backup (Windows)'],
    ['старая резервная копия iOS', 'old iOS backup'],
    ['файл резервной копии iOS', 'iOS backup file'],
    ['Time Machine образ', 'Time Machine image'],
    ['неиспользуемая локализация (.lproj)', 'unused localization (.lproj)'],
    ['файл локализации Windows (MUI)', 'Windows localization file (MUI)'],
    ['файл локализации Linux', 'Linux localization file']
  ];
  let translated = text;
  for (const [source, target] of phrases) translated = translated.replace(source, target);
  return translated;
}

export function translateDom(root = document) {
  applying = true;
  try {
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      if (!originalText.has(node) && /[А-Яа-яЁё]/.test(node.nodeValue || '')) {
        originalText.set(node, node.nodeValue);
      }
      if (originalText.has(node)) {
        const source = originalText.get(node);
        const next = currentLanguage === 'ru' ? source : tr(source);
        if (node.nodeValue !== next) node.nodeValue = next;
      }
    }

    const elements = root.querySelectorAll ? [root, ...root.querySelectorAll('*')] : [];
    for (const element of elements) {
      if (!(element instanceof Element)) continue;
      let stored = originalAttributes.get(element);
      for (const name of ATTRIBUTES) {
        if (!element.hasAttribute(name)) continue;
        const value = element.getAttribute(name);
        if (!stored && /[А-Яа-яЁё]/.test(value)) {
          stored = {};
          originalAttributes.set(element, stored);
        }
        if (stored && stored[name] == null && /[А-Яа-яЁё]/.test(value)) stored[name] = value;
        if (stored && stored[name] != null) {
          element.setAttribute(name, currentLanguage === 'ru' ? stored[name] : tr(stored[name]));
        }
      }
    }
  } finally {
    applying = false;
  }
}

export function setLanguage(language, { persist = true } = {}) {
  currentLanguage = language === 'en' ? 'en' : 'ru';
  if (persist) localStorage.setItem(STORAGE_KEY, currentLanguage);
  document.documentElement.lang = currentLanguage;
  translateDom(document);
  document.querySelectorAll('[data-language]').forEach(button => {
    const active = button.dataset.language === currentLanguage;
    button.classList.toggle('active', active);
    button.setAttribute('aria-pressed', String(active));
  });
  window.dispatchEvent(new CustomEvent('app-language-change', { detail: currentLanguage }));
}

export function initLanguageToggle() {
  document.querySelectorAll('[data-language]').forEach(button => {
    button.addEventListener('click', () => setLanguage(button.dataset.language));
  });
  setLanguage(currentLanguage, { persist: false });

  const observer = new MutationObserver(() => {
    if (!applying) translateDom(document);
  });
  observer.observe(document.body, { childList: true, subtree: true });
}
