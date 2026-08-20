// unused-removal Web UI - Main Application
// This file is embedded and served by the Rust binary

// ===== Types =====
const Category = {
  HUGE: 'huge',
  LARGE: 'large',
  JUNK: 'junk',
  OLD_LOG: 'old_log',
  STALE_INSTALL: 'stale_install',
  STALE: 'stale',
  DUPLICATE: 'duplicate',
  APP_LEFTOVERS: 'app_leftovers',
  // Smart Junk categories
  USER_CACHE: 'user_cache',
  SYSTEM_LOG: 'system_log',
  LANGUAGE_FILE: 'language_file',
  OLD_BACKUP: 'old_backup',
  MAIL_ATTACHMENT: 'mail_attachment',
  TRASH: 'trash',
  OLD_DOWNLOAD: 'old_download',
  UNUSED_DISK_IMAGE: 'unused_disk_image',
  DEV_CACHE: 'dev_cache',
  XCODE_CACHE: 'xcode_cache',
  VSCODE_CACHE: 'vscode_cache',
  LARGE_HIDDEN: 'large_hidden'
};

const Risk = {
  SAFE: 'safe',
  CAUTION: 'caution',
  PROTECTED: 'protected'
};

// Smart Junk Safety Levels
const SafetyLevel = {
  SAFE: 'safe',
  BALANCED: 'balanced',
  AGGRESSIVE: 'aggressive'
};

// Category icons mapping
const CATEGORY_ICONS = {
  huge: '🔴',
  large: '🟠',
  junk: '🗑',
  old_log: '📄',
  stale_install: '📦',
  stale: '⏳',
  duplicate: '🔁',
  app_leftovers: '📦',
  user_cache: '💾',
  system_log: '📋',
  language_file: '🌐',
  old_backup: '💿',
  mail_attachment: '📎',
  trash: '🗑️',
  old_download: '⬇️',
  unused_disk_image: '💿',
  dev_cache: '⚙️',
  xcode_cache: '🛠️',
  vscode_cache: '💻',
  large_hidden: '🔍'
};

// Category descriptions
const CATEGORY_DESCRIPTIONS = {
  huge: 'Очень крупные файлы',
  large: 'Крупные файлы',
  junk: 'Временные и мусорные файлы',
  old_log: 'Старые лог-файлы',
  stale_install: 'Старые инсталляторы',
  stale: 'Не используемые давно',
  duplicate: 'Дубликаты',
  app_leftovers: 'Следы удалённых приложений',
  user_cache: 'Кэш браузеров и приложений',
  system_log: 'Системные и приложения логи',
  language_file: 'Неиспользуемые локализации',
  old_backup: 'Старые бэкапы (iOS, Time Machine, Windows)',
  mail_attachment: 'Старые вложения почты',
  trash: 'Корзина / Recycle Bin',
  old_download: 'Старые файлы в Загрузках',
  unused_disk_image: 'Неиспользуемые образы дисков',
  dev_cache: 'Кэш инструментов разработки (npm, cargo, pip, gradle)',
  xcode_cache: 'Xcode DerivedData, Archives, DeviceSupport',
  vscode_cache: 'VS Code / Cursor кэш и логи',
  large_hidden: 'Большие скрытые файлы'
};

// Safety level descriptions
const SAFETY_DESCRIPTIONS = {
  safe: 'Только кэши, логи, корзина, старые загрузки — максимальная безопасность',
  balanced: '+ языки, бэкапы, вложения почты — рекомендуемый баланс',
  aggressive: 'Всё включённое + образы дисков, скрытые файлы, дубликаты — максимальное освобождение'
};

// Categories allowed per safety level
const SAFETY_CATEGORIES = {
  safe: ['junk', 'user_cache', 'system_log', 'trash', 'old_download', 'dev_cache', 'xcode_cache', 'vscode_cache', 'old_log', 'stale_install'],
  balanced: ['junk', 'user_cache', 'system_log', 'trash', 'old_download', 'dev_cache', 'xcode_cache', 'vscode_cache', 'old_log', 'stale_install', 'language_file', 'old_backup', 'mail_attachment'],
  aggressive: ['junk', 'user_cache', 'system_log', 'trash', 'old_download', 'dev_cache', 'xcode_cache', 'vscode_cache', 'old_log', 'stale_install', 'language_file', 'old_backup', 'mail_attachment', 'unused_disk_image', 'large_hidden', 'stale', 'duplicate', 'app_leftovers', 'huge', 'large']
};

// ===== State =====
const state = {
  phase: 'smart-scan',
  scanId: 0,
  findings: [],
  filteredFindings: [],
  selectedPaths: new Set(),
  currentPage: 1,
  pageSize: 100,
  totalPages: 1,
  sort: { key: 'size', dir: 'desc' },
  filters: { category: '', search: '' },
  pendingDeleteMode: null,
  // Smart scan state
  smartScanCategories: [],
  smartSelectedCategories: new Set(),
  smartSafetyLevel: 'balanced',
  smartTotalReclaimable: 0
};

// ===== DOM Elements =====
const els = {};

// ===== Animation Helpers =====
function animatePhaseTransition(fromEl, toEl, callback) {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (prefersReduced || !fromEl) {
    fromEl?.classList.add('hidden');
    toEl.classList.remove('hidden');
    callback?.();
    return;
  }

  fromEl.classList.add('exiting');
  fromEl.addEventListener('animationend', function handler() {
    fromEl.removeEventListener('animationend', handler);
    fromEl.classList.add('hidden');
    fromEl.classList.remove('exiting');
    toEl.classList.remove('hidden');
    toEl.classList.add('entering');
    toEl.addEventListener('animationend', function handler2() {
      toEl.removeEventListener('animationend', handler2);
      toEl.classList.remove('entering');
      callback?.();
    }, { once: true });
  }, { once: true });
}

function staggerChildren(container, selector, baseDelay = 30, staggerDelay = 20) {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (prefersReduced) return;

  const items = container.querySelectorAll(selector);
  items.forEach((el, i) => {
    el.style.animationDelay = `${baseDelay + i * staggerDelay}ms`;
  });
}

function triggerRowAnimations(tbody) {
  staggerChildren(tbody, 'tr', 0, 15);
}

function triggerStatAnimations() {
  staggerChildren(document.querySelector('.progress-stats-grid') || document.body, '.stat-item', 50, 50);
}

// ===== Utility Functions =====
function formatBytes(bytes) {
  const units = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  if (bytes === 0) return '0 B';
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1) + ' ' + units[i];
}

function formatNumber(n) {
  return n.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ' ');
}

function formatDate(isoString) {
  const date = new Date(isoString);
  return date.toLocaleString('ru-RU', {
    year: 'numeric', month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit'
  });
}

function categoryLabel(cat) {
  const labels = {
    huge: 'Очень крупные', large: 'Крупные', junk: 'Мусор',
    old_log: 'Старые логи', stale_install: 'Старые инсталляторы',
    stale: 'Не использовались', duplicate: 'Дубликаты',
    app_leftovers: 'Следы приложений'
  };
  return labels[cat] || cat;
}

// ===== Professional icon set (Lucide-style inline SVG, no external deps) =====
const ICON_PATHS = {
  huge: '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
  large: '<path d="M14.5 17.5 3 6V3h3l11.5 11.5"/><path d="M13 19l6-6"/><path d="M16 16l4 4"/><path d="M19 21l2-2"/>',
  junk: '<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><line x1="10" y1="11" x2="10" y2="17"/><line x1="14" y1="11" x2="14" y2="17"/>',
  old_log: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/><line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/>',
  stale_install: '<path d="M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z"/><polyline points="3.27 6.96 12 12.01 20.73 6.96"/><line x1="12" y1="22.08" x2="12" y2="12"/>',
  stale: '<circle cx="12" cy="12" r="10"/><polyline points="12 6 12 12 16 14"/>',
  duplicate: '<rect x="9" y="9" width="13" height="13" rx="2" ry="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>',
  file: '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/>',
  folder: '<path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/>',
  pencil: '<path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/>',
  trash: '<polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
  check_circle: '<path d="M22 11.08V12a10 10 0 1 1-5.93-9.14"/><polyline points="22 4 12 14.01 9 11.01"/>',
  alert: '<path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>',
  octagon: '<polygon points="7.86 2 16.14 2 22 7.86 22 16.14 16.14 22 7.86 22 2 16.14 2 7.86 7.86 2"/><line x1="12" y1="8" x2="12" y2="12"/><line x1="12" y1="16" x2="12.01" y2="16"/>',
  app_leftovers: '<path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 17h-2v-2h2v2zm2.07-7.75l-1.29 1.29-2.58-2.58-1.29 1.29L11 15.67l-2.58-2.58-1.29 1.29L11 18.16l4.77-4.77 1.29 1.29L14.54 14z"/>',
};

function iconSvg(name, size = 16, cls = '') {
  const body = ICON_PATHS[name] || ICON_PATHS.file;
  return `<svg class="icon ${cls}" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
}

function categoryIcon(cat) {
  return iconSvg(cat, 15, `cat-icon cat-${cat || 'file'}`);
}

function riskLabel(risk) {
  const labels = { safe: 'Безопасно', caution: 'Осторожно', protected: 'Защищено' };
  return labels[risk] || risk;
}

function riskColor(risk) {
  const colors = { safe: '#22c55e', caution: '#f59e0b', protected: '#ef4444' };
  return colors[risk] || '#6b7280';
}

function riskClass(risk) {
  return `risk-${risk}`;
}

function escapeHtml(text) {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

function debounce(fn, ms) {
  let timer;
  return (...args) => { clearTimeout(timer); timer = setTimeout(() => fn(...args), ms); };
}

function clamp(n, min, max) { return Math.max(min, Math.min(max, n)); }

// ===== API Client =====
const API_BASE = '/api';

async function request(endpoint, options = {}) {
  const res = await fetch(`${API_BASE}${endpoint}`, {
    headers: { 'Content-Type': 'application/json', ...options.headers },
    ...options
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`API error ${res.status}: ${text}`);
  }
  if (res.status === 204) return;
  return res.json();
}

const api = {
  startScan: (config) => request('/scan', { method: 'POST', body: JSON.stringify(config) }),
  stopScan: () => request('/stop', { method: 'POST' }),
  getProgress: () => request('/progress'),
  getResults: (params) => {
    const qs = new URLSearchParams();
    Object.entries(params).forEach(([k, v]) => { if (v !== undefined) qs.set(k, v); });
    return request(`/results?${qs.toString()}`);
  },
  deleteFiles: (payload) => request('/delete', { method: 'POST', body: JSON.stringify(payload) }),
  getConfig: () => request('/config'),
  saveConfig: (config) => request('/config', { method: 'PUT', body: JSON.stringify(config) }),
  exportReport: async (format) => {
    const res = await fetch(`${API_BASE}/export?format=${format}`);
    if (!res.ok) throw new Error(`Export failed: ${res.statusText}`);
    return res.blob();
  },
  // Smart Scan API
  startSmartScan: () => request('/smart-scan', { method: 'POST' }),
  getSmartCategories: () => request('/smart-categories'),
  smartClean: (categories, mode) => request('/smart-clean', { method: 'POST', body: JSON.stringify({ categories, mode }) })
};

// ===== Initialization =====
function init() {
  cacheElements();
  bindEvents();
  loadConfig();
}

function cacheElements() {
  // Config phase
  els.rootSelect = document.getElementById('root-select');
  els.rootCustom = document.getElementById('root-custom');
  els.workers = document.getElementById('workers');
  els.followLinks = document.getElementById('follow-links');
  els.useCache = document.getElementById('use-cache');
  els.checkDuplicates = document.getElementById('check-duplicates');
  els.protectSystem = document.getElementById('protect-system');
  els.btnScan = document.getElementById('btn-scan');
  els.btnStop = document.getElementById('btn-stop');
  els.btnBackToSmart = document.getElementById('btn-back-to-smart');

  // Smart Scan phase
  els.smartScanSection = document.getElementById('smart-scan-phase');
  els.smartProgressSection = document.getElementById('smart-scan-progress');
  els.smartProgressRingFill = document.querySelector('#smart-progress-ring-fill');
  els.smartProgressPercent = document.getElementById('smart-progress-percent');
  els.smartProgressDesc = document.getElementById('smart-progress-desc');
  els.smartSafetyLevel = document.getElementById('smart-safety-level');
  els.btnSmartScan = document.getElementById('btn-smart-scan');
  els.btnSmartAdvanced = document.getElementById('btn-smart-advanced');

  // Progress phase
  els.progressSection = document.getElementById('progress-phase');
  els.resultsSection = document.getElementById('results-phase');
  els.smartResultsSection = document.getElementById('smart-results-phase');
  els.configSection = document.getElementById('config-phase');
  els.progressRingFill = document.querySelector('#progress-ring-fill');
  els.progressPercent = document.getElementById('progress-percent');
  els.statFiles = document.getElementById('stat-files');
  els.statDirs = document.getElementById('stat-dirs');
  els.statBytes = document.getElementById('stat-bytes');
  els.statRate = document.getElementById('stat-rate');
  els.statElapsed = document.getElementById('stat-elapsed');
  els.statCached = document.getElementById('stat-cached');
  els.statCurrent = document.getElementById('stat-current');
  els.recentFiles = document.getElementById('recent-files');
  els.btnStopProgress = document.getElementById('btn-stop-progress');

  // Smart Results phase
  els.smartSummaryTotal = document.getElementById('smart-summary-total');
  els.smartSummarySize = document.getElementById('smart-summary-size');
  els.smartResultsSafety = document.getElementById('smart-results-safety');
  els.categoryCards = document.getElementById('category-cards');
  els.btnSmartCleanAll = document.getElementById('btn-smart-clean-all');
  els.btnSmartReview = document.getElementById('btn-smart-review');
  els.donutReclaimable = document.getElementById('donut-reclaimable-text');
  els.donutUsed = document.getElementById('donut-used');
  els.donutReclaimableSeg = document.getElementById('donut-reclaimable');

  // Results phase
  els.filterCategory = document.getElementById('filter-category');
  els.filterSearch = document.getElementById('filter-search');
  els.resultsBody = document.getElementById('results-body');
  els.tableWrapper = document.querySelector('.table-wrapper');
  els.selectAll = document.getElementById('select-all');
  els.selCount = document.getElementById('sel-count');
  els.selSize = document.getElementById('sel-size');
  els.summaryTotal = document.getElementById('summary-total');
  els.summaryCats = document.getElementById('summary-cats');
  els.pagination = document.getElementById('pagination');
  els.btnPrev = document.getElementById('btn-prev');
  els.btnNext = document.getElementById('btn-next');
  els.pageInfo = document.getElementById('page-info');
  els.btnRecycle = document.getElementById('btn-recycle');
  els.btnHard = document.getElementById('btn-hard');
  els.btnExportJson = document.getElementById('btn-export-json');
  els.btnExportCsv = document.getElementById('btn-export-csv');
  els.selectionSummary = document.getElementById('selection-summary');

  // Modal
  els.modal = document.getElementById('modal');
  els.modalTitle = document.getElementById('modal-title');
  els.modalText = document.getElementById('modal-text');
  els.modalCancel = document.getElementById('modal-cancel');
  els.modalConfirm = document.getElementById('modal-confirm');
}

function bindEvents() {
  els.rootSelect.addEventListener('change', () => {
    const show = els.rootSelect.value === 'custom';
    els.rootCustom.style.display = show ? 'block' : 'none';
    if (show) els.rootCustom.focus();
  });

  // Smart Scan events
  els.btnSmartScan.addEventListener('click', startSmartScan);
  els.btnSmartAdvanced.addEventListener('click', () => setPhase('config'));
  els.smartSafetyLevel.addEventListener('change', (e) => {
    state.smartSafetyLevel = e.target.value;
  });
  els.smartResultsSafety.addEventListener('change', (e) => {
    state.smartSafetyLevel = e.target.value;
    applySmartSafetyFilter();
  });
  els.btnSmartCleanAll.addEventListener('click', executeSmartClean);
  els.btnSmartReview.addEventListener('click', () => setPhase('results'));

  els.btnScan.addEventListener('click', startScan);
  els.btnStop.addEventListener('click', stopScan);
  if (els.btnStopProgress) els.btnStopProgress.addEventListener('click', stopScan);
  if (els.btnBackToSmart) els.btnBackToSmart.addEventListener('click', () => setPhase('smart-scan'));

  els.filterCategory.addEventListener('change', applyFilters);
  els.filterSearch.addEventListener('input', debounce(applyFilters, 300));

  els.selectAll.addEventListener('change', toggleSelectAll);

  document.querySelectorAll('th[data-sort]').forEach(th => {
    th.addEventListener('click', () => {
      const key = th.getAttribute('data-sort');
      if (state.sort.key === key) state.sort.dir = state.sort.dir === 'asc' ? 'desc' : 'asc';
      else { state.sort.key = key; state.sort.dir = 'desc'; }
      applyFilters();
    });
  });

  els.btnPrev.addEventListener('click', () => changePage(-1));
  els.btnNext.addEventListener('click', () => changePage(1));

  els.btnRecycle.addEventListener('click', () => confirmDelete('recycle'));
  els.btnHard.addEventListener('click', () => confirmDelete('hard'));
  els.btnExportJson.addEventListener('click', () => exportReport('json'));
  els.btnExportCsv.addEventListener('click', () => exportReport('csv'));

  els.modalCancel.addEventListener('click', hideModal);
  els.modalConfirm.addEventListener('click', executeDelete);
  els.modal.addEventListener('click', (e) => { if (e.target === els.modal) hideModal(); });

  // Keyboard shortcuts
  document.addEventListener('keydown', (e) => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
    if (state.phase === 'results' || state.phase === 'smart-results') {
      if (e.key === ' ') { e.preventDefault(); toggleRowSelection(); }
      else if (e.key === 't') confirmDelete('recycle');
      else if (e.key === 'x') confirmDelete('hard');
      else if (e.key === 'c') cycleCategoryFilter();
      else if (e.key === 'r') setPhase('config');
    }
    if (e.key === 'Escape') {
      if (state.phase === 'confirm') hideModal();
      else if (state.phase === 'scanning') stopScan();
    }
  });

  // Button press feedback
  document.querySelectorAll('.btn').forEach(btn => {
    btn.addEventListener('mousedown', () => { if (!btn.disabled) btn.style.transform = 'scale(0.98)'; });
    btn.addEventListener('mouseup', () => { if (!btn.disabled) btn.style.transform = ''; });
    btn.addEventListener('mouseleave', () => { btn.style.transform = ''; });
  });
}

async function loadConfig() {
  try {
    const cfg = await api.getConfig();
    if (cfg.workers) els.workers.value = cfg.workers;
    els.followLinks.checked = cfg.follow_links ?? false;
    els.useCache.checked = cfg.use_cache ?? true;
    els.checkDuplicates.checked = cfg.check_duplicates ?? false;
    els.protectSystem.checked = cfg.protect_system ?? true;

    populateRootPaths(cfg);
  } catch (e) {
    console.warn('Config load failed:', e);
    detectDrivesFallback();
  }
}

function populateRootPaths(cfg) {
  const paths = cfg.default_paths && cfg.default_paths.length > 0
    ? cfg.default_paths
    : (cfg.os === 'macos' || cfg.os === 'linux' ? ['/', '.'] : ['C:\\', 'D:\\']);

  els.rootSelect.innerHTML = paths.map(p => `<option value="${escapeHtml(p)}">${escapeHtml(p)}</option>`).join('');

  const opt = document.createElement('option');
  opt.value = 'custom';
  opt.textContent = 'Указать свой путь…';
  els.rootSelect.appendChild(opt);

  if (cfg.root && paths.includes(cfg.root)) {
    els.rootSelect.value = cfg.root;
  } else if (cfg.root && cfg.root !== 'C:\\') {
    els.rootSelect.value = 'custom';
    els.rootCustom.hidden = false;
    els.rootCustom.value = cfg.root;
  }
}

function detectDrivesFallback() {
  const isUnix = !navigator.userAgent.includes('Windows');
  const defaults = isUnix ? ['/', '.'] : ['C:\\', 'D:\\', 'E:\\'];
  els.rootSelect.innerHTML = defaults.map(d => `<option value="${d}">${d}</option>`).join('');
  const opt = document.createElement('option');
  opt.value = 'custom';
  opt.textContent = 'Указать свой путь…';
  els.rootSelect.appendChild(opt);
}

// ===== Phase Management =====
function setPhase(phase) {
  const prevPhase = state.phase;
  state.phase = phase;
  const isScanning = phase === 'scanning';
  const isResults = phase === 'results';
  const isSmartResults = phase === 'smart-results';
  const isSmartScan = phase === 'smart-scan';
  const isDeleting = phase === 'deleting';

  els.btnScan.disabled = isScanning || isDeleting;
  els.btnStop.disabled = !isScanning;
  if (els.btnStopProgress) els.btnStopProgress.disabled = !isScanning;

  const sections = {
    config: els.configSection,
    scanning: els.progressSection,
    results: els.resultsSection,
    'smart-scan': els.smartScanSection,
    'smart-results': els.smartResultsSection
  };

  const prevSection = sections[prevPhase];
  const nextSection = sections[phase];

  if (prevSection && nextSection && prevSection !== nextSection) {
    animatePhaseTransition(prevSection, nextSection);
  } else {
    Object.values(sections).forEach(s => s?.classList.add('hidden'));
    nextSection?.classList.remove('hidden');
  }

  if (isScanning) {
    const ring = els.progressRingFill;
    if (ring) {
      const CIRC = 2 * Math.PI * 49;
      ring.style.strokeDasharray = String(CIRC);
      ring.style.strokeDashoffset = String(CIRC);
    }
    if (els.progressPercent) els.progressPercent.textContent = '0';
    triggerStatAnimations();
  }

  if (isResults) {
    triggerRowAnimations(els.resultsBody);
  }

  if (isSmartResults) {
    // Initialize safety selector
    els.smartResultsSafety.value = state.smartSafetyLevel;
    renderCategoryCards();
    animateDonutChart();
  }

  if (isSmartScan) {
    // Reset smart scan progress
    els.smartProgressSection.classList.add('hidden');
    els.btnSmartScan.disabled = false;
    els.smartProgressPercent.textContent = '0';
    els.smartProgressDesc.textContent = 'Подготовка…';
    const ring = els.smartProgressRingFill;
    if (ring) {
      const CIRC = 2 * Math.PI * 49;
      ring.style.strokeDasharray = String(CIRC);
      ring.style.strokeDashoffset = String(CIRC);
    }
  }
}

// ===== Config Phase =====
async function startScan() {
  const root = els.rootSelect.value === 'custom' ? els.rootCustom.value.trim() : els.rootSelect.value;
  if (!root) { showToast('Выберите или введите путь для сканирования', 'warning'); return; }

  const config = {
    root,
    workers: parseInt(els.workers.value) || 0,
    follow_links: els.followLinks.checked,
    use_cache: els.useCache.checked,
    check_duplicates: els.checkDuplicates.checked,
    protect_system: els.protectSystem.checked
  };

  api.saveConfig(config).catch(e => console.warn('Config save failed:', e));

  setPhase('scanning');
  state.currentPage = 1;
  state.selectedPaths.clear();

  try {
    const res = await api.startScan(config);
    state.scanId = res.scan_id;
    pollProgress();
  } catch (e) {
    const msg = e && e.message ? e.message : String(e);
    const hint = msg.includes('Failed to fetch')
      ? 'Сервер недоступен. Запустите: unused-removal serve'
      : msg;
    showToast('Ошибка запуска: ' + hint, 'error');
    setPhase('config');
  }
}

function stopScan() {
  api.stopScan().then(() => {
    showToast('Сканирование остановлено', 'info');
    setPhase('config');
  }).catch(e => showToast('Ошибка остановки: ' + e.message, 'error'));
}

// ===== Scanning Phase =====
let progressPollTimer = null;

function pollProgress() {
  if (progressPollTimer) clearTimeout(progressPollTimer);
  api.getProgress().then(data => {
    updateProgress(data.progress);
    if (!data.done) progressPollTimer = setTimeout(pollProgress, 500);
    else loadResults();
  }).catch(e => {
    console.error('Progress poll error:', e);
    progressPollTimer = setTimeout(pollProgress, 1000);
  });
}

let lastRecentPaths = new Set();

function updateProgress(p) {
  els.statFiles.textContent = formatNumber(p.files);
  els.statDirs.textContent = formatNumber(p.dirs);
  els.statBytes.textContent = formatBytes(p.bytes);
  els.statRate.textContent = p.rate_fps ? `${formatNumber(Math.round(p.rate_fps))} ф/с` : '—';
  els.statElapsed.textContent = p.remain_s > 0
    ? `${formatDuration(p.elapsed_s)} / ~${formatDuration(p.remain_s)}`
    : formatDuration(p.elapsed_s);
  els.statCached.textContent = formatNumber(p.cached);
  els.statCurrent.textContent = p.current || '—';

  const ring = els.progressRingFill;
  if (ring) {
    const CIRC = 2 * Math.PI * 49;
    ring.style.strokeDasharray = String(CIRC);
    if (p.finished) { ring.style.strokeDashoffset = '0'; els.progressPercent.textContent = '100'; }
    else if (p.percent >= 0) {
      const frac = clamp(p.percent, 0, 100) / 100;
      ring.style.strokeDashoffset = String(CIRC * (1 - frac));
      els.progressPercent.textContent = String(Math.round(p.percent));
    } else { ring.style.strokeDashoffset = String(CIRC); els.progressPercent.textContent = '…'; }
  }

  updateRecentFiles(p.recent || []);
}

function formatDuration(seconds) {
  if (seconds < 60) return seconds.toFixed(1) + ' с';
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  if (m < 60) return `${m} мин ${s} с`;
  return `${Math.floor(m / 60)} ч ${m % 60} мин`;
}

function updateRecentFiles(recent) {
  const el = els.recentFiles;
  if (!el) return;
  if (recent.length === 0) { el.innerHTML = '<span class="recent-empty">Ещё нет файлов…</span>'; return; }

  const newPaths = recent.slice(-30).reverse();
  const fragments = newPaths.map((p, i) => {
    const isNew = !lastRecentPaths.has(p);
    if (isNew) lastRecentPaths.add(p);
    return `<div class="recent-file${isNew ? ' new' : ''}" style="animation-delay: ${i * 30}ms">${escapeHtml(p)}</div>`;
  }).join('');

  el.innerHTML = fragments;
}

// ===== Results Phase =====
async function loadResults() {
  try {
    const res = await api.getResults({ limit: 10000 });
    state.findings = res.items;
    state.filteredFindings = [...res.items];
    state.currentPage = 1;
    lastRecentPaths.clear();
    renderTable();
    updateSummary();
    setPhase('results');
  } catch (e) {
    showToast('Ошибка загрузки результатов: ' + e.message, 'error');
    setPhase('config');
  }
}

// ===== Smart Scan Phase =====
let smartProgressPollTimer = null;

async function startSmartScan() {
  const root = els.rootSelect.value === 'custom' ? els.rootCustom.value.trim() : els.rootSelect.value;
  if (!root) { showToast('Выберите или введите путь для сканирования', 'warning'); return; }

  // Update config with smart scan settings
  const config = {
    root,
    workers: parseInt(els.workers.value) || 0,
    follow_links: els.followLinks.checked,
    use_cache: els.useCache.checked,
    check_duplicates: true, // Always enable for smart scan
    protect_system: els.protectSystem.checked,
    // Enable all smart junk categories
    smart_junk_enabled: true,
    scan_user_caches: true,
    scan_system_logs: true,
    scan_language_files: true,
    scan_old_backups: true,
    scan_mail_attachments: true,
    scan_trash: true,
    scan_old_downloads: true,
    scan_unused_disk_images: true,
    scan_dev_caches: true,
    scan_ide_caches: true,
    scan_large_hidden: true,
    smart_junk_safety_level: state.smartSafetyLevel
  };

  api.saveConfig(config).catch(e => console.warn('Config save failed:', e));

  // Show progress UI
  els.smartProgressSection.classList.remove('hidden');
  els.btnSmartScan.disabled = true;
  state.currentPage = 1;
  state.selectedPaths.clear();
  state.smartSelectedCategories.clear();

  try {
    const res = await api.startSmartScan();
    state.scanId = res.scan_id;
    pollSmartProgress();
  } catch (e) {
    const msg = e && e.message ? e.message : String(e);
    const hint = msg.includes('Failed to fetch')
      ? 'Сервер недоступен. Запустите: unused-removal serve'
      : msg;
    showToast('Ошибка запуска: ' + hint, 'error');
    setPhase('smart-scan');
  }
}

function pollSmartProgress() {
  if (smartProgressPollTimer) clearTimeout(smartProgressPollTimer);
  api.getProgress().then(data => {
    updateSmartProgress(data.progress);
    if (!data.done) smartProgressPollTimer = setTimeout(pollSmartProgress, 500);
    else loadSmartResults();
  }).catch(e => {
    console.error('Smart progress poll error:', e);
    smartProgressPollTimer = setTimeout(pollSmartProgress, 1000);
  });
}

function updateSmartProgress(p) {
  els.smartProgressDesc.textContent = p.current || 'Сканирование…';
  
  const ring = els.smartProgressRingFill;
  if (ring) {
    const CIRC = 2 * Math.PI * 49;
    ring.style.strokeDasharray = String(CIRC);
    if (p.finished) { 
      ring.style.strokeDashoffset = '0'; 
      els.smartProgressPercent.textContent = '100'; 
    }
    else if (p.percent >= 0) {
      const frac = clamp(p.percent, 0, 100) / 100;
      ring.style.strokeDashoffset = String(CIRC * (1 - frac));
      els.smartProgressPercent.textContent = String(Math.round(p.percent));
    } else { 
      ring.style.strokeDashoffset = String(CIRC); 
      els.smartProgressPercent.textContent = '…'; 
    }
  }
}

async function loadSmartResults() {
  try {
    const res = await api.getSmartCategories();
    state.smartScanCategories = res.categories;
    state.smartTotalReclaimable = res.total_reclaimable;
    state.smartSelectedCategories = new Set(res.categories.map(c => c.category));
    state.findings = []; // Will be populated when user clicks "Review"
    
    // Update summary
    els.smartSummaryTotal.textContent = formatNumber(res.total_files);
    els.smartSummarySize.textContent = formatBytes(res.total_reclaimable);
    
    // Update donut chart text
    els.donutReclaimable.textContent = formatBytes(res.total_reclaimable);
    
    // Render category cards
    renderCategoryCards();
    animateDonutChart();
    
    setPhase('smart-results');
  } catch (e) {
    showToast('Ошибка загрузки результатов: ' + e.message, 'error');
    setPhase('smart-scan');
  }
}

function renderCategoryCards() {
  const container = els.categoryCards;
  if (!container) return;
  
  const allowedCategories = SAFETY_CATEGORIES[state.smartSafetyLevel] || SAFETY_CATEGORIES.balanced;
  
  container.innerHTML = state.smartScanCategories.map((cat, i) => {
    const isAllowed = allowedCategories.includes(cat.category);
    const isSelected = state.smartSelectedCategories.has(cat.category);
    const icon = CATEGORY_ICONS[cat.category] || '📄';
    const description = CATEGORY_DESCRIPTIONS[cat.category] || cat.description;
    const riskClass = cat.risk === 'safe' ? 'safe' : cat.risk === 'caution' ? 'caution' : 'protected';
    
    return `
<div class="category-card${isSelected ? ' selected' : ''}" data-category="${escapeHtml(cat.category)}" style="animation-delay: ${50 + i * 30}ms; opacity: ${isAllowed ? '1' : '0.5'};">
  <div class="category-card-header">
    <div class="category-icon">${icon}</div>
    <div class="category-info">
      <div class="category-name">${escapeHtml(cat.category)}</div>
      <div class="category-count">${formatNumber(cat.count)} файлов · ${formatBytes(cat.total_size)}</div>
    </div>
    <span class="category-risk ${riskClass}">${cat.risk === 'safe' ? 'Безопасно' : cat.risk === 'caution' ? 'Осторожно' : 'Защищено'}</span>
  </div>
  <div class="category-stats">
    <span class="category-size">${formatBytes(cat.total_size)}</span>
    <label class="category-checkbox">
      <input type="checkbox" ${isSelected ? 'checked' : ''} ${!isAllowed ? 'disabled' : ''} onchange="toggleSmartCategory('${escapeHtml(cat.category)}', this.checked)">
      <span>Выбрать</span>
    </label>
  </div>
  <div class="category-paths">
    ${cat.paths_sample.slice(0, 3).map(p => `<div class="category-path" title="${escapeHtml(p)}">${escapeHtml(p)}</div>`).join('')}
    ${cat.count > 3 ? `<div class="category-path" style="color: var(--accent); cursor: pointer;" onclick="toggleCategoryExpand(this.closest('.category-card'))">… и ещё ${cat.count - 3} файлов</div>` : ''}
  </div>
  <button class="category-toggle" onclick="toggleCategoryExpand(this.closest('.category-card'))" aria-label="Развернуть">
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="6 9 12 15 18 9"/></svg>
  </button>
</div>
`;
  }).join('');
  
  // Apply stagger animation
  staggerChildren(container, '.category-card', 50, 30);
}

function animateDonutChart() {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (prefersReduced) {
    // Set final values immediately
    const usedSeg = document.getElementById('donut-used');
    const reclaimSeg = document.getElementById('donut-reclaimable');
    const totalSize = state.smartScanCategories.reduce((sum, c) => sum + c.total_size, 0);
    const reclaimable = state.smartTotalReclaimable;
    const used = totalSize - reclaimable;
    const circumference = 2 * Math.PI * 80;
    
    if (usedSeg) usedSeg.style.strokeDashoffset = circumference * (1 - used / totalSize);
    if (reclaimSeg) reclaimSeg.style.strokeDashoffset = circumference * (1 - reclaimable / totalSize);
    return;
  }
  
  // Animate
  setTimeout(() => {
    const usedSeg = document.getElementById('donut-used');
    const reclaimSeg = document.getElementById('donut-reclaimable');
    const totalSize = state.smartScanCategories.reduce((sum, c) => sum + c.total_size, 0);
    const reclaimable = state.smartTotalReclaimable;
    const used = totalSize - reclaimable;
    const circumference = 2 * Math.PI * 80;
    
    if (usedSeg) usedSeg.style.strokeDashoffset = circumference * (1 - used / totalSize);
    if (reclaimSeg) reclaimSeg.style.strokeDashoffset = circumference * (1 - reclaimable / totalSize);
  }, 100);
}

function toggleSmartCategory(category, checked) {
  if (checked) {
    state.smartSelectedCategories.add(category);
  } else {
    state.smartSelectedCategories.delete(category);
  }
  // Update card visual
  const card = document.querySelector(`.category-card[data-category="${category}"]`);
  if (card) card.classList.toggle('selected', checked);
  
  // Update clean button state
  const hasSelection = state.smartSelectedCategories.size > 0;
  els.btnSmartCleanAll.disabled = !hasSelection;
}

function toggleCategoryExpand(card) {
  if (!card) return;
  card.classList.toggle('expanded');
}

function applySmartSafetyFilter() {
  // Re-render cards with new safety level
  renderCategoryCards();
}

async function executeSmartClean() {
  if (state.smartSelectedCategories.size === 0) return;
  
  const categories = Array.from(state.smartSelectedCategories);
  const mode = 'recycle'; // Default to recycle bin
  
  // Show confirmation modal
  const totalSize = state.smartScanCategories
    .filter(c => state.smartSelectedCategories.has(c.category))
    .reduce((sum, c) => sum + c.total_size, 0);
  
  const count = state.smartScanCategories
    .filter(c => state.smartSelectedCategories.has(c.category))
    .reduce((sum, c) => sum + c.count, 0);
  
  els.modalTitle.innerHTML = `<svg class="modal-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg> Очистка в Корзину`;
  els.modalText.innerHTML = `
Удалить <strong>${formatNumber(count)}</strong> файлов 
(<strong>${formatBytes(totalSize)}</strong>) в <strong>${categories.length}</strong> категориях?<br><br>
Файлы можно будет восстановить из Корзины.
  `;
  els.modalConfirm.innerHTML = `<svg class="btn-icon" viewBox="0 0 24 24" width="15" height="15" fill="none" stroke="currentColor" stroke-width="2"><polyline points="3 6 5 6 21 6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg> В Корзину`;
  els.modalConfirm.className = 'btn btn-primary';
  els.modal.classList.remove('hidden');
  els.modalConfirm.focus();
  
  // Override executeDelete for smart clean
  els.modalConfirm.onclick = async () => {
    els.modal.classList.add('hidden');
    state.pendingDeleteMode = null;
    
    try {
      const res = await api.smartClean(categories, mode);
      showToast(`Готово: удалено ${res.deleted}, ошибок ${res.failed}`, res.success ? 'success' : 'warning');
      state.smartSelectedCategories.clear();
      loadSmartResults(); // Refresh
    } catch (e) {
      showToast('Ошибка очистки: ' + e.message, 'error');
    }
  };
}

function applyFilters() {
  const cat = els.filterCategory.value;
  const search = els.filterSearch.value.toLowerCase();
  state.filters.category = cat;
  state.filters.search = search;

  state.filteredFindings = state.findings.filter(f => {
    if (cat && f.category !== cat) return false;
    if (search) {
      const path = f.path.toLowerCase();
      const reason = f.reason.toLowerCase();
      if (!path.includes(search) && !reason.includes(search)) return false;
    }
    return true;
  });

  // Sort
  const { key, dir } = state.sort;
  if (key) {
    const mul = dir === 'asc' ? 1 : -1;
    state.filteredFindings.sort((a, b) => {
      switch (key) {
        case 'size': return (a.size - b.size) * mul;
        case 'category': return a.category.localeCompare(b.category) * mul;
        case 'mod_time': return (new Date(a.mod_time).getTime() - new Date(b.mod_time).getTime()) * mul;
        case 'risk': return a.risk.localeCompare(b.risk) * mul;
        case 'path': default: return a.path.localeCompare(b.path) * mul;
      }
    });
  }

  state.currentPage = 1;
  renderTable();
  updateSummary();
  updateSortIndicators();
}

function updateSortIndicators() {
  document.querySelectorAll('th[data-sort]').forEach(th => {
    const key = th.getAttribute('data-sort');
    const arrow = state.sort.key === key ? (state.sort.dir === 'asc' ? ' ↑' : ' ↓') : '';
    const label = th.querySelector('.sort-label');
    if (label) label.textContent = arrow;
  });
}

function renderTable() {
  const start = (state.currentPage - 1) * state.pageSize;
  const end = start + state.pageSize;
  const pageItems = state.filteredFindings.slice(start, end);

  els.resultsBody.innerHTML = pageItems.map((f, i) => {
    const checked = state.selectedPaths.has(f.path) ? 'checked' : '';
    const risk = f.risk;
    const selectedClass = state.selectedPaths.has(f.path) ? ' selected' : '';
    return `
<tr data-path="${escapeHtml(f.path)}" class="${selectedClass}" style="--risk-color: ${riskColor(risk)}; animation-delay: ${i * 15}ms">
  <td><input type="checkbox" class="row-check" ${checked} data-path="${escapeHtml(f.path)}"></td>
  <td class="path-cell" title="${escapeHtml(f.path)}">
    <span class="file-icon">${categoryIcon(f.category)}</span>
    <span class="file-name">${escapeHtml(f.path)}</span>
  </td>
  <td>${formatBytes(f.size)}</td>
  <td><span class="cat-badge cat-${f.category}">${categoryIcon(f.category)} ${categoryLabel(f.category)}</span></td>
  <td>${escapeHtml(f.reason)}</td>
  <td><span class="risk-badge ${riskClass(risk)}">${riskLabel(risk)}</span></td>
  <td>${formatDate(f.mod_time)}</td>
</tr>
`;
  }).join('');

  document.querySelectorAll('.row-check').forEach(cb => {
    cb.addEventListener('change', (e) => {
      const path = e.target.dataset.path;
      if (e.target.checked) state.selectedPaths.add(path);
      else state.selectedPaths.delete(path);
      updateSelectionUI();
      const row = e.target.closest('tr');
      if (row) row.classList.toggle('selected', e.target.checked);
    });
  });

  updatePagination();
  updateSelectionUI();
}

function updatePagination() {
  state.totalPages = Math.max(1, Math.ceil(state.filteredFindings.length / state.pageSize));
  els.pageInfo.textContent = `Стр. ${state.currentPage} / ${state.totalPages}`;
  els.btnPrev.disabled = state.currentPage <= 1;
  els.btnNext.disabled = state.currentPage >= state.totalPages;
}

function changePage(delta) {
  state.currentPage = clamp(state.currentPage + delta, 1, state.totalPages);
  renderTable();
  // Scroll to table top smoothly
  els.tableWrapper?.scrollTo({ top: 0, behavior: 'smooth' });
}

function toggleSelectAll(e) {
  const target = e.target;
  const pageItems = state.filteredFindings.slice(
    (state.currentPage - 1) * state.pageSize,
    state.currentPage * state.pageSize
  );
  pageItems.forEach(f => {
    if (target.checked) state.selectedPaths.add(f.path);
    else state.selectedPaths.delete(f.path);
  });
  renderTable();
}

function updateSelectionUI() {
  let totalSize = 0;
  state.filteredFindings.forEach(f => { if (state.selectedPaths.has(f.path)) totalSize += f.size; });
  els.selCount.textContent = formatNumber(state.selectedPaths.size);
  els.selSize.textContent = formatBytes(totalSize);
  const hasSelection = state.selectedPaths.size > 0;
  els.btnRecycle.disabled = !hasSelection;
  els.btnHard.disabled = !hasSelection;
  els.selectionSummary.hidden = !hasSelection;
}

function updateSummary() {
  const byCat = {};
  state.findings.forEach(f => { byCat[f.category] = (byCat[f.category] || 0) + 1; });
  const parts = Object.entries(byCat).map(([cat, cnt]) => `${categoryIcon(cat)} ${categoryLabel(cat)}: ${cnt}`).join(' • ');
  if (els.summaryTotal) els.summaryTotal.textContent = String(state.findings.length);
  if (els.summaryCats) els.summaryCats.innerHTML = parts || 'пусто';
}

function cycleCategoryFilter() {
  const cats = ['', 'huge', 'large', 'junk', 'old_log', 'stale_install', 'stale', 'duplicate'];
  const idx = cats.indexOf(state.filters.category);
  state.filters.category = cats[(idx + 1) % cats.length];
  els.filterCategory.value = state.filters.category;
  applyFilters();
}

function toggleRowSelection() {
  const selectedRow = document.querySelector('#results-table tbody tr.selected');
  if (selectedRow) {
    const path = selectedRow.dataset.path;
    const checkbox = selectedRow.querySelector('.row-check');
    if (checkbox) { checkbox.checked = !checkbox.checked; checkbox.dispatchEvent(new Event('change')); }
  }
}

// ===== Delete Actions =====
function confirmDelete(mode) {
  if (state.selectedPaths.size === 0) return;
  state.pendingDeleteMode = mode;
  const isHard = mode === 'hard';
  els.modalTitle.innerHTML = (isHard ? iconSvg('octagon', 18, 'modal-icon danger') : iconSvg('trash', 18, 'modal-icon')) + ' ' +
    (isHard ? 'Безвозвратное удаление' : 'Перемещение в Корзину');
  const totalSize = Array.from(state.selectedPaths).reduce((sum, p) => {
    const f = state.findings.find(x => x.path === p);
    return sum + (f?.size || 0);
  }, 0);
  els.modalText.innerHTML = `
Удалить <strong>${formatNumber(state.selectedPaths.size)}</strong> файлов
(<strong>${formatBytes(totalSize)}</strong>)?<br><br>
${isHard ? iconSvg('alert', 16, 'modal-icon danger') + ' <strong>Это действие НЕОБРАТИМО!</strong> Файлы не попадут в Корзину.' : iconSvg('check_circle', 16, 'modal-icon success') + ' Файлы можно будет восстановить из Корзины.'}
  `;
  els.modalConfirm.innerHTML = (isHard ? iconSvg('octagon', 15, 'btn-icon') : iconSvg('trash', 15, 'btn-icon')) + ' ' + (isHard ? 'Удалить навсегда' : 'В Корзину');
  els.modalConfirm.className = isHard ? 'btn danger' : 'btn primary';
  els.modal.classList.remove('hidden');
  els.modalConfirm.focus();
}

function hideModal() {
  const prefersReduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  if (!prefersReduced) {
    els.modal.classList.add('exiting');
    els.modal.addEventListener('animationend', () => {
      els.modal.classList.add('hidden');
      els.modal.classList.remove('exiting');
    }, { once: true });
  } else {
    els.modal.classList.add('hidden');
  }
  state.pendingDeleteMode = null;
}

async function executeDelete() {
  if (!state.pendingDeleteMode) return;
  const mode = state.pendingDeleteMode;
  const paths = Array.from(state.selectedPaths);
  hideModal();
  setPhase('deleting');

  try {
    const res = await api.deleteFiles({ paths, mode });
    showToast(`Готово: удалено ${res.deleted}, ошибок ${res.failed}`, res.success ? 'success' : 'warning');
    state.selectedPaths.clear();
    state.findings = state.findings.filter(f => !paths.includes(f.path));
    applyFilters();
    setPhase('results');
  } catch (e) {
    showToast('Ошибка удаления: ' + e.message, 'error');
    setPhase('results');
  }
}

// ===== Export =====
async function exportReport(format) {
  try {
    const blob = await api.exportReport(format);
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url; a.download = `unused-removal-report.${format}`; a.click();
    URL.revokeObjectURL(url);
    showToast(`Отчёт ${format.toUpperCase()} скачан`, 'success');
  } catch (e) { showToast('Ошибка экспорта: ' + e.message, 'error'); }
}

// ===== Toast =====
function showToast(message, type = 'info') {
  const container = document.getElementById('toast-container');
  const toast = document.createElement('div');
  toast.className = `toast toast-${type}`;
  toast.textContent = message;
  container.appendChild(toast);

  // Trigger entrance animation
  requestAnimationFrame(() => toast.classList.add('show'));

  setTimeout(() => {
    toast.classList.add('leaving');
    toast.addEventListener('animationend', () => toast.remove(), { once: true });
  }, 3000);
}

// ===== Start =====
document.addEventListener('DOMContentLoaded', init);

// Expose for debugging
window.app = { startScan, stopScan, confirmDelete, hideModal };