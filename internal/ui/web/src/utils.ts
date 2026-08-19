// Утилиты

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const sizes = ['B', 'KiB', 'MiB', 'GiB', 'TiB', 'PiB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString('ru-RU', {
    day: '2-digit',
    month: '2-digit',
    year: 'numeric',
  }) + ' ' + d.toLocaleTimeString('ru-RU', {
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatNumber(n: number): string {
  return n.toLocaleString('ru-RU');
}

export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)} с`;
  const m = Math.floor(seconds / 60);
  const s = Math.floor(seconds % 60);
  if (m < 60) return `${m} мин ${s} с`;
  const h = Math.floor(m / 60);
  return `${h} ч ${m % 60} мин`;
}

export function categoryLabel(cat: string): string {
  const map: Record<string, string> = {
    huge: 'Очень крупный',
    large: 'Крупный',
    junk: 'Мусор',
    old_log: 'Старый лог',
    stale_install: 'Старый инсталлятор',
    stale: 'Не использовался давно',
    duplicate: 'Дубликат',
  };
  return map[cat] || cat;
}

export function riskLabel(risk: string): string {
  const map: Record<string, string> = {
    safe: 'Безопасно',
    caution: 'С осторожностью',
    protected: 'Защищён',
  };
  return map[risk] || risk;
}

export function categoryIcon(cat: string): string {
  const map: Record<string, string> = {
    huge: '🔴',
    large: '🟠',
    junk: '🗑',
    old_log: '📄',
    stale_install: '📦',
    stale: '⏳',
    duplicate: '🔁',
  };
  return map[cat] || '📁';
}

export function riskColor(risk: string): string {
  const map: Record<string, string> = {
    safe: '#9ece6a',
    caution: '#e0af68',
    protected: '#f7768e',
  };
  return map[risk] || '#787c99';
}

export function escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

export function debounce<T extends (...args: unknown[]) => void>(
  fn: T,
  ms: number
): T {
  let timer: ReturnType<typeof setTimeout>;
  return ((...args: unknown[]) => {
    clearTimeout(timer);
    timer = setTimeout(() => fn(...args), ms);
  }) as T;
}

export function throttle<T extends (...args: unknown[]) => void>(
  fn: T,
  ms: number
): T {
  let last = 0;
  return ((...args: unknown[]) => {
    const now = Date.now();
    if (now - last >= ms) {
      last = now;
      fn(...args);
    }
  }) as T;
}

export function clamp(n: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, n));
}