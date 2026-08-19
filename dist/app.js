// Главный класс приложения
import { api } from './api';
import { formatBytes, formatDate, formatNumber, formatDuration, categoryLabel, riskLabel, categoryIcon, riskColor, escapeHtml, debounce, clamp, } from './utils';
// Состояние приложения
const state = {
    phase: 'config',
    scanId: 0,
    findings: [],
    filteredFindings: [],
    selectedPaths: new Set(),
    currentPage: 1,
    pageSize: 100,
    totalPages: 1,
    sort: { key: 'size', dir: 'desc' },
    filters: { category: '', search: '' },
};
// DOM элементы
const els = {
    // Config phase
    rootSelect: null,
    rootCustom: null,
    workers: null,
    followLinks: null,
    useCache: null,
    checkDuplicates: null,
    protectSystem: null,
    btnScan: null,
    btnStop: null,
    // Progress phase
    progressSection: null,
    resultsSection: null,
    progressRingFill: null,
    progressPercent: null,
    statFiles: null,
    statDirs: null,
    statBytes: null,
    statRate: null,
    statElapsed: null,
    statCached: null,
    statCurrent: null,
    recentFiles: null,
    btnStopProgress: null,
    // Results phase
    filterCategory: null,
    filterSearch: null,
    resultsBody: null,
    selectAll: null,
    selCount: null,
    selSize: null,
    summaryTotal: null,
    summaryCats: null,
    pagination: null,
    btnPrev: null,
    btnNext: null,
    pageInfo: null,
    btnRecycle: null,
    btnHard: null,
    btnExportJson: null,
    btnExportCsv: null,
    // Modal
    modal: null,
    modalTitle: null,
    modalText: null,
    modalCancel: null,
    modalConfirm: null,
};
// Инициализация
export function init() {
    cacheElements();
    bindEvents();
    loadConfig();
    detectDrives();
}
function cacheElements() {
    // Config
    els.rootSelect = document.getElementById('root-select');
    els.rootCustom = document.getElementById('root-custom');
    els.workers = document.getElementById('workers');
    els.followLinks = document.getElementById('follow-links');
    els.useCache = document.getElementById('use-cache');
    els.checkDuplicates = document.getElementById('check-duplicates');
    els.protectSystem = document.getElementById('protect-system');
    els.btnScan = document.getElementById('btn-scan');
    els.btnStop = document.getElementById('btn-stop');
    // Progress
    els.progressSection = document.getElementById('progress-phase');
    els.resultsSection = document.getElementById('results-phase');
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
    // Results
    els.filterCategory = document.getElementById('filter-category');
    els.filterSearch = document.getElementById('filter-search');
    els.resultsBody = document.getElementById('results-body');
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
    // Modal
    els.modal = document.getElementById('modal');
    els.modalTitle = document.getElementById('modal-title');
    els.modalText = document.getElementById('modal-text');
    els.modalCancel = document.getElementById('modal-cancel');
    els.modalConfirm = document.getElementById('modal-confirm');
}
function bindEvents() {
    // Config
    els.rootSelect.addEventListener('change', () => {
        els.rootCustom.style.display =
            els.rootSelect.value === 'custom' ? 'block' : 'none';
    });
    els.btnScan.addEventListener('click', startScan);
    els.btnStop.addEventListener('click', stopScan);
    if (els.btnStopProgress) {
        els.btnStopProgress.addEventListener('click', stopScan);
    }
    // Filters
    els.filterCategory.addEventListener('change', applyFilters);
    els.filterSearch.addEventListener('input', debounce(applyFilters, 300));
    // Selection
    els.selectAll.addEventListener('change', toggleSelectAll);
    // Sorting
    document.querySelectorAll('th[data-sort]').forEach((th) => {
        th.addEventListener('click', () => {
            const key = th.getAttribute('data-sort');
            if (state.sort.key === key) {
                state.sort.dir = state.sort.dir === 'asc' ? 'desc' : 'asc';
            }
            else {
                state.sort.key = key;
                state.sort.dir = 'desc';
            }
            applyFilters();
        });
    });
    // Pagination
    els.btnPrev.addEventListener('click', () => changePage(-1));
    els.btnNext.addEventListener('click', () => changePage(1));
    // Actions
    els.btnRecycle.addEventListener('click', () => confirmDelete('recycle'));
    els.btnHard.addEventListener('click', () => confirmDelete('hard'));
    els.btnExportJson.addEventListener('click', () => exportReport('json'));
    els.btnExportCsv.addEventListener('click', () => exportReport('csv'));
    // Modal
    els.modalCancel.addEventListener('click', hideModal);
    els.modalConfirm.addEventListener('click', executeDelete);
    els.modal.addEventListener('click', (e) => {
        if (e.target === els.modal)
            hideModal();
    });
}
// ===== Config Phase =====
async function loadConfig() {
    try {
        const cfg = await api.getConfig();
        if (cfg.workers)
            els.workers.value = String(cfg.workers);
        els.followLinks.checked = cfg.follow_links ?? false;
        els.useCache.checked = cfg.use_cache ?? true;
        els.checkDuplicates.checked = cfg.check_duplicates ?? false;
        els.protectSystem.checked = cfg.protect_system ?? true;
    }
    catch (e) {
        console.warn('Config load failed:', e);
    }
}
function detectDrives() {
    const drives = ['C:\\', 'D:\\', 'E:\\', 'F:\\', 'G:\\'];
    els.rootSelect.innerHTML = drives
        .map((d) => `<option value="${d}">${d}</option>`)
        .join('');
    // Добавляем опцию "Custom"
    const opt = document.createElement('option');
    opt.value = 'custom';
    opt.textContent = '📝 Указать свой путь...';
    els.rootSelect.appendChild(opt);
}
async function startScan() {
    const root = els.rootSelect.value === 'custom'
        ? els.rootCustom.value.trim()
        : els.rootSelect.value;
    if (!root) {
        showToast('Выберите или введите путь для сканирования', 'warning');
        return;
    }
    const config = {
        root,
        workers: parseInt(els.workers.value) || 0,
        follow_links: els.followLinks.checked,
        use_cache: els.useCache.checked,
        check_duplicates: els.checkDuplicates.checked,
        protect_system: els.protectSystem.checked,
    };
    // Сохраняем конфиг
    try {
        await api.saveConfig({
            ...(await api.getConfig()),
            ...config,
        });
    }
    catch (e) {
        console.warn('Config save failed:', e);
    }
    setPhase('scanning');
    state.currentPage = 1;
    state.selectedPaths.clear();
    try {
        const res = await api.startScan(config);
        state.scanId = res.scan_id;
        pollProgress();
    }
    catch (e) {
        showToast('Ошибка запуска: ' + e.message, 'error');
        setPhase('config');
    }
}
function stopScan() {
    api
        .stopScan()
        .then(() => {
        showToast('Сканирование остановлено', 'info');
        setPhase('config');
    })
        .catch((e) => {
        showToast('Ошибка остановки: ' + e.message, 'error');
    });
}
// ===== Scanning Phase =====
let progressPollTimer = null;
function pollProgress() {
    if (progressPollTimer)
        clearTimeout(progressPollTimer);
    api
        .getProgress()
        .then((data) => {
        updateProgress(data.progress);
        if (!data.done) {
            progressPollTimer = setTimeout(pollProgress, 500);
        }
        else {
            loadResults();
        }
    })
        .catch((e) => {
        console.error('Progress poll error:', e);
        progressPollTimer = setTimeout(pollProgress, 1000);
    });
}
function updateProgress(p) {
    els.statFiles.textContent = formatNumber(p.files);
    els.statDirs.textContent = formatNumber(p.dirs);
    els.statBytes.textContent = formatBytes(p.bytes);
    els.statRate.textContent = p.rate_fps
        ? `${formatNumber(Math.round(p.rate_fps))} ф/с`
        : '—';
    els.statElapsed.textContent = p.remain_s > 0
        ? `${formatDuration(p.elapsed_s)} / ~${formatDuration(p.remain_s)}`
        : formatDuration(p.elapsed_s);
    els.statCached.textContent = formatNumber(p.cached);
    els.statCurrent.textContent = p.current || '—';
    // Кольцо прогресса: процент из оценки общего числа файлов (кэш прошлого скана).
    // Если оценка неизвестна (percent < 0) — показываем живой счётчик вместо кольца.
    const ring = els.progressRingFill;
    if (ring) {
        const CIRC = 2 * Math.PI * 54;
        ring.style.strokeDasharray = String(CIRC);
        if (p.finished) {
            ring.style.strokeDashoffset = '0';
            els.progressPercent.textContent = '100';
        }
        else if (p.percent >= 0) {
            const frac = clamp(p.percent, 0, 100) / 100;
            ring.style.strokeDashoffset = String(CIRC * (1 - frac));
            els.progressPercent.textContent = String(Math.round(p.percent));
        }
        else {
            // Оценка неизвестна — кольцо пустое, показываем «…».
            ring.style.strokeDashoffset = String(CIRC);
            els.progressPercent.textContent = '…';
        }
    }
    // Живой список последних обработанных файлов
    updateRecentFiles(p.recent || []);
}
function updateRecentFiles(recent) {
    const el = els.recentFiles;
    if (!el)
        return;
    if (recent.length === 0) {
        el.innerHTML = '<span class="recent-empty">Ещё нет файлов…</span>';
        return;
    }
    // Показываем последние (в обратном порядке — свежие сверху)
    el.innerHTML = recent
        .slice(-30)
        .reverse()
        .map((path) => `<div class="recent-file">${escapeHtml(path)}</div>`)
        .join('');
}
// ===== Results Phase =====
async function loadResults() {
    try {
        const res = await api.getResults({
            limit: 10000, // load all for client-side filtering
        });
        state.findings = res.items;
        state.filteredFindings = [...res.items];
        state.currentPage = 1;
        renderTable();
        updateSummary();
        setPhase('results');
    }
    catch (e) {
        showToast('Ошибка загрузки результатов: ' + e.message, 'error');
        setPhase('config');
    }
}
function applyFilters() {
    const cat = els.filterCategory.value;
    const search = els.filterSearch.value.toLowerCase();
    state.filters.category = cat;
    state.filters.search = search;
    state.filteredFindings = state.findings.filter((f) => {
        if (cat && f.category !== cat)
            return false;
        if (search) {
            const path = f.path.toLowerCase();
            const reason = f.reason.toLowerCase();
            if (!path.includes(search) && !reason.includes(search))
                return false;
        }
        return true;
    });
    // Сортировка
    const { key, dir } = state.sort;
    if (key) {
        const mul = dir === 'asc' ? 1 : -1;
        state.filteredFindings.sort((a, b) => {
            switch (key) {
                case 'size':
                    return (a.size - b.size) * mul;
                case 'category':
                    return a.category.localeCompare(b.category) * mul;
                case 'mod_time':
                    return (new Date(a.mod_time).getTime() - new Date(b.mod_time).getTime()) * mul;
                case 'risk':
                    return a.risk.localeCompare(b.risk) * mul;
                case 'path':
                default:
                    return a.path.localeCompare(b.path) * mul;
            }
        });
    }
    state.currentPage = 1;
    renderTable();
    updateSummary();
    updateSortIndicators();
}
function updateSortIndicators() {
    document.querySelectorAll('th[data-sort]').forEach((th) => {
        const key = th.getAttribute('data-sort');
        const arrow = state.sort.key === key ? (state.sort.dir === 'asc' ? ' ↑' : ' ↓') : '';
        const label = th.querySelector('.sort-label');
        if (label)
            label.textContent = arrow;
    });
}
function renderTable() {
    const start = (state.currentPage - 1) * state.pageSize;
    const end = start + state.pageSize;
    const pageItems = state.filteredFindings.slice(start, end);
    els.resultsBody.innerHTML = pageItems
        .map((f, i) => {
        const globalIdx = start + i;
        const checked = state.selectedPaths.has(f.path) ? 'checked' : '';
        const risk = f.risk;
        return `
<tr data-path="${escapeHtml(f.path)}" style="--risk-color: ${riskColor(risk)}">
  <td><input type="checkbox" class="row-check" ${checked} data-path="${escapeHtml(f.path)}"></td>
  <td class="path-cell" title="${escapeHtml(f.path)}">
    <span class="file-icon">${categoryIcon(f.category)}</span>
    <span class="file-name">${escapeHtml(f.path)}</span>
  </td>
  <td>${formatBytes(f.size)}</td>
  <td><span class="cat-badge cat-${f.category}">${categoryIcon(f.category)} ${categoryLabel(f.category)}</span></td>
  <td>${escapeHtml(f.reason)}</td>
  <td><span class="risk-badge" style="background: ${riskColor(risk)}22; color: ${riskColor(risk)}; border-color: ${riskColor(risk)}44;">${riskLabel(risk)}</span></td>
  <td>${formatDate(f.mod_time)}</td>
</tr>
`;
    })
        .join('');
    // Привязка чекбоксов
    document.querySelectorAll('.row-check').forEach((cb) => {
        cb.addEventListener('change', (e) => {
            const target = e.target;
            const path = target.dataset.path;
            if (target.checked)
                state.selectedPaths.add(path);
            else
                state.selectedPaths.delete(path);
            updateSelectionUI();
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
}
function toggleSelectAll(e) {
    const target = e.target;
    const pageItems = state.filteredFindings.slice((state.currentPage - 1) * state.pageSize, state.currentPage * state.pageSize);
    pageItems.forEach((f) => {
        if (target.checked)
            state.selectedPaths.add(f.path);
        else
            state.selectedPaths.delete(f.path);
    });
    renderTable();
}
function updateSelectionUI() {
    let totalSize = 0;
    state.filteredFindings.forEach((f) => {
        if (state.selectedPaths.has(f.path))
            totalSize += f.size;
    });
    els.selCount.textContent = formatNumber(state.selectedPaths.size);
    els.selSize.textContent = formatBytes(totalSize);
    const hasSelection = state.selectedPaths.size > 0;
    els.btnRecycle.disabled = !hasSelection;
    els.btnHard.disabled = !hasSelection;
}
function updateSummary() {
    const byCat = {};
    state.findings.forEach((f) => {
        byCat[f.category] = (byCat[f.category] || 0) + 1;
    });
    const parts = Object.entries(byCat)
        .map(([cat, cnt]) => `${categoryIcon(cat)} ${categoryLabel(cat)}: ${cnt}`)
        .join(' • ');
    if (els.summaryTotal)
        els.summaryTotal.textContent = String(state.findings.length);
    if (els.summaryCats)
        els.summaryCats.textContent = parts || 'пусто';
}
// ===== Delete Actions =====
let pendingDeleteMode = null;
function confirmDelete(mode) {
    if (state.selectedPaths.size === 0)
        return;
    pendingDeleteMode = mode;
    const isHard = mode === 'hard';
    els.modalTitle.textContent = isHard
        ? '⚠️ Безвозвратное удаление'
        : '🗑 Перемещение в Корзину';
    els.modalText.innerHTML = `
Удалить <strong>${formatNumber(state.selectedPaths.size)}</strong> файлов 
(<strong>${formatBytes(Array.from(state.selectedPaths)
        .map((p) => state.findings.find((f) => f.path === p)?.size || 0)
        .reduce((a, b) => a + b, 0))}</strong>)?<br><br>
${isHard ? '⚠️ <strong>Это действие НЕОБРАТИМО!</strong> Файлы не попадут в Корзину.' : '✅ Файлы можно будет восстановить из Корзины.'}
  `;
    els.modalConfirm.textContent = isHard ? '💀 Удалить навсегда' : '🗑 В Корзину';
    els.modalConfirm.className = isHard ? 'btn danger' : 'btn primary';
    els.modal.classList.remove('hidden');
    els.modalConfirm.focus();
}
function hideModal() {
    els.modal.classList.add('hidden');
    pendingDeleteMode = null;
}
async function executeDelete() {
    if (!pendingDeleteMode)
        return;
    const mode = pendingDeleteMode;
    const paths = Array.from(state.selectedPaths);
    hideModal();
    setPhase('deleting');
    try {
        const res = await api.deleteFiles({ paths, mode });
        showToast(`Готово: удалено ${res.deleted}, ошибок ${res.failed}`, res.success ? 'success' : 'warning');
        // Удаляем из списка
        state.selectedPaths.clear();
        state.findings = state.findings.filter((f) => !paths.includes(f.path));
        applyFilters();
        setPhase('results');
    }
    catch (e) {
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
        a.href = url;
        a.download = `unused-removal-report.${format}`;
        a.click();
        URL.revokeObjectURL(url);
        showToast(`Отчёт ${format.toUpperCase()} скачан`, 'success');
    }
    catch (e) {
        showToast('Ошибка экспорта: ' + e.message, 'error');
    }
}
// ===== UI Helpers =====
function setPhase(phase) {
    state.phase = phase;
    const isScanning = phase === 'scanning';
    const isResults = phase === 'results';
    const isDeleting = phase === 'deleting';
    els.btnScan.disabled = isScanning || isDeleting;
    els.btnStop.disabled = !isScanning;
    if (els.btnStopProgress)
        els.btnStopProgress.disabled = !isScanning;
    els.progressSection.classList.toggle('hidden', !isScanning && !isDeleting);
    els.resultsSection.classList.toggle('hidden', !isResults);
    if (isScanning) {
        const ring = els.progressRingFill;
        if (ring) {
            const CIRC = 2 * Math.PI * 54;
            ring.style.strokeDasharray = String(CIRC);
            ring.style.strokeDashoffset = String(CIRC);
        }
        if (els.progressPercent)
            els.progressPercent.textContent = '0';
    }
}
function showToast(message, type = 'info') {
    const toast = document.createElement('div');
    toast.className = `toast toast-${type}`;
    toast.textContent = message;
    document.body.appendChild(toast);
    requestAnimationFrame(() => toast.classList.add('show'));
    setTimeout(() => {
        toast.classList.remove('show');
        setTimeout(() => toast.remove(), 300);
    }, 3000);
}
// Глобальные функции для onclick в HTML (если нужно)
window.app = { startScan, stopScan, confirmDelete, hideModal };
// Запуск
document.addEventListener('DOMContentLoaded', init);
//# sourceMappingURL=app.js.map