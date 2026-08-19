// API клиент

import { Category, Risk, Finding as TypesFinding } from './types';

const API_BASE = '/api';

export interface API {
  startScan(config: ScanConfig): Promise<{ scan_id: number; status: string }>;
  stopScan(): Promise<{ status: string; stopped: boolean }>;
  getProgress(): Promise<{ progress: ScanProgress; done: boolean }>;
  getResults(params: ResultsParams): Promise<ResultsResponse>;
  deleteFiles(request: DeleteRequestPayload): Promise<DeleteResponse>;
  getConfig(): Promise<Config>;
  saveConfig(config: Config): Promise<{ status: string }>;
  exportReport(format: 'json' | 'csv'): Promise<Blob>;
}

export interface ScanConfig {
  root: string;
  workers: number;
  follow_links: boolean;
  use_cache: boolean;
  check_duplicates: boolean;
  protect_system: boolean;
}

export interface ScanProgress {
  files: number;
  dirs: number;
  bytes: number;
  errors: number;
  cached: number;
  total: number;
  percent: number;
  current: string;
  elapsed_s: number;
  rate_fps: number;
  remain_s: number;
  finished: boolean;
  recent: string[];
}

export interface Finding {
  path: string;
  size: number;
  category: Category;
  reason: string;
  risk: Risk;
  mod_time: string;
  extra?: Record<string, string>;
}

export interface ResultsParams {
  category?: string;
  search?: string;
  limit?: number;
  offset?: number;
}

export interface ResultsResponse {
  total: number;
  filtered: number;
  items: Finding[];
}

export interface DeleteRequestPayload {
  paths: string[];
  mode: 'recycle' | 'hard';
}

export interface DeleteResponse {
  deleted: number;
  failed: number;
  total_bytes: number;
  errors: Array<{ path: string; error: string }>;
  success: boolean;
}

export interface Config {
  root: string;
  workers: number;
  follow_links: boolean;
  use_cache: boolean;
  check_duplicates: boolean;
  protect_system: boolean;
  large_bytes: number;
  huge_bytes: number;
  stale_days: number;
  old_log_days: number;
  stale_install_days: number;
  junk_extensions: string[];
  junk_dirs: string[];
  exclude_dirs: string[];
  exclude_prefix: string[];
  allow_protected: boolean;
}

async function request<T>(
  endpoint: string,
  options: RequestInit = {}
): Promise<T> {
  const res = await fetch(`${API_BASE}${endpoint}`, {
    headers: {
      'Content-Type': 'application/json',
      ...options.headers,
    },
    ...options,
  });

  if (!res.ok) {
    const text = await res.text();
    throw new Error(`API error ${res.status}: ${text}`);
  }

  if (res.status === 204) return undefined as T;
  return res.json();
}

export const api: API = {
  startScan(config) {
    return request('/scan', {
      method: 'POST',
      body: JSON.stringify(config),
    });
  },

  stopScan() {
    return request('/stop', { method: 'POST' });
  },

  getProgress() {
    return request('/progress');
  },

  getResults(params) {
    const qs = new URLSearchParams();
    if (params.category) qs.set('category', params.category);
    if (params.search) qs.set('search', params.search);
    if (params.limit) qs.set('limit', String(params.limit));
    if (params.offset) qs.set('offset', String(params.offset));
    return request(`/results?${qs.toString()}`);
  },

  deleteFiles(payload: DeleteRequestPayload) {
    return request('/delete', {
      method: 'POST',
      body: JSON.stringify(payload),
    });
  },

  getConfig() {
    return request('/config');
  },

  saveConfig(config) {
    return request('/config', {
      method: 'PUT',
      body: JSON.stringify(config),
    });
  },

  async exportReport(format) {
    const res = await fetch(`${API_BASE}/export?format=${format}`);
    if (!res.ok) throw new Error(`Export failed: ${res.statusText}`);
    return res.blob();
  },
};