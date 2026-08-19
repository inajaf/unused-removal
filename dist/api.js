// API клиент
const API_BASE = '/api';
async function request(endpoint, options = {}) {
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
    if (res.status === 204)
        return undefined;
    return res.json();
}
export const api = {
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
        if (params.category)
            qs.set('category', params.category);
        if (params.search)
            qs.set('search', params.search);
        if (params.limit)
            qs.set('limit', String(params.limit));
        if (params.offset)
            qs.set('offset', String(params.offset));
        return request(`/results?${qs.toString()}`);
    },
    deleteFiles(payload) {
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
        if (!res.ok)
            throw new Error(`Export failed: ${res.statusText}`);
        return res.blob();
    },
};
//# sourceMappingURL=api.js.map