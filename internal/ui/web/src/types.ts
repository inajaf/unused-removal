// Типы для API и UI

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

export type Category =
  | 'huge'
  | 'large'
  | 'junk'
  | 'old_log'
  | 'stale_install'
  | 'stale'
  | 'duplicate';

export type Risk = 'safe' | 'caution' | 'protected';

export interface ScanStartResponse {
  scan_id: number;
  status: string;
}

export interface ProgressResponse {
  progress: ScanProgress;
  done: boolean;
}

export interface ResultsResponse {
  total: number;
  filtered: number;
  items: Finding[];
}

export interface DeleteRequest {
  paths: string[];
  mode: 'recycle' | 'hard';
}

export interface DeleteResponse {
  deleted: number;
  failed: number;
  total_bytes: number;
  errors: DeleteError[];
  success: boolean;
}

export interface DeleteError {
  path: string;
  error: string;
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

export interface ExportResponse {
  format: 'json' | 'csv';
}

// UI State
export type State = UIState;

export interface UIState {
  phase: 'config' | 'scanning' | 'results' | 'deleting';
  scanId: number;
  findings: Finding[];
  filteredFindings: Finding[];
  selectedPaths: Set<string>;
  currentPage: number;
  pageSize: number;
  totalPages: number;
  sort: {
    key: SortKey;
    dir: 'asc' | 'desc';
  };
  filters: {
    category: string;
    search: string;
  };
}

export type SortKey = 'path' | 'size' | 'category' | 'mod_time' | 'risk' | null;

export interface FindingsResponse {
  total: number;
  filtered: number;
  items: Finding[];
}