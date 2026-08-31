const fs = require('fs');
const path = require('path');
const { chromium } = require('playwright');

const root = path.resolve(__dirname, '..');
const output = path.join(__dirname, 'demo_states');
fs.mkdirSync(output, { recursive: true });

const config = {
  os: 'windows',
  root: 'C:\\',
  default_paths: ['C:\\', 'D:\\'],
  workers: 0,
  follow_links: false,
  use_cache: true,
  check_duplicates: false,
  protect_system: true,
  large_bytes: 1073741824,
  huge_bytes: 10737418240,
  stale_days: 365,
  old_log_days: 30,
  stale_install_days: 120,
  junk_extensions: ['.tmp', '.bak', '.old'],
  junk_dirs: [],
  exclude_dirs: [],
  exclude_prefix: [],
  allow_protected: false
};

const gib = 1024 ** 3;
const mib = 1024 ** 2;
const findings = [
  { path: 'C:\\Users\\Alex\\Videos\\archive_2024.mkv', size: 12.8 * gib, category: 'huge', reason: 'Very large file — review before removal', risk: 'caution', mod_time: '2024-04-12T09:24:00Z' },
  { path: 'C:\\hiberfil.sys', size: 8 * gib, category: 'huge', reason: 'Protected Windows system file', risk: 'protected', mod_time: '2026-08-27T08:10:00Z' },
  { path: 'C:\\Users\\Alex\\Downloads\\Windows11.iso', size: 5.8 * gib, category: 'unused_disk_image', reason: 'Unused disk image — review before removal', risk: 'caution', mod_time: '2024-11-03T18:42:00Z' },
  { path: 'C:\\Users\\Alex\\Downloads\\camera_backup.zip', size: 4.1 * gib, category: 'large', reason: 'Large archive — review before removal', risk: 'caution', mod_time: '2025-01-18T12:02:00Z' },
  { path: 'C:\\Users\\Alex\\AppData\\Local\\Google\\Chrome\\User Data\\Default\\Cache\\data_3', size: 820 * mib, category: 'user_cache', reason: 'Browser cache', risk: 'safe', mod_time: '2026-08-25T14:20:00Z' },
  { path: 'C:\\Users\\Alex\\AppData\\Local\\Temp\\render-cache.tmp', size: 610 * mib, category: 'junk', reason: 'Temporary file', risk: 'safe', mod_time: '2026-08-22T10:10:00Z' },
  { path: 'C:\\Users\\Alex\\Downloads\\setup-old.exe', size: 540 * mib, category: 'stale_install', reason: 'Old installer in Downloads', risk: 'caution', mod_time: '2023-09-14T07:16:00Z' },
  { path: 'C:\\Users\\Alex\\AppData\\Local\\Microsoft\\Edge\\User Data\\Default\\Cache\\data_1', size: 430 * mib, category: 'user_cache', reason: 'Browser cache', risk: 'safe', mod_time: '2026-08-26T13:14:00Z' },
  { path: 'C:\\Users\\Alex\\AppData\\Local\\Temp\\update.log', size: 190 * mib, category: 'old_log', reason: 'Old log file', risk: 'safe', mod_time: '2025-02-02T16:34:00Z' }
];

const categories = [
  { category: 'huge', count: 4, total_size: 20.8 * gib, risk: 'protected', description: 'Very large files', paths_sample: ['C:\\hiberfil.sys', 'C:\\Users\\Alex\\Videos\\archive_2024.mkv', 'C:\\Users\\Alex\\VMs\\Windows-dev.vhdx'] },
  { category: 'large', count: 23, total_size: 14.6 * gib, risk: 'caution', description: 'Large files', paths_sample: ['C:\\Users\\Alex\\Downloads\\camera_backup.zip', 'C:\\Users\\Alex\\Videos\\recording.mp4', 'C:\\Users\\Alex\\Documents\\archive.7z'] },
  { category: 'unused_disk_image', count: 6, total_size: 8.2 * gib, risk: 'caution', description: 'Unused disk images', paths_sample: ['C:\\Users\\Alex\\Downloads\\Windows11.iso', 'C:\\Users\\Alex\\Downloads\\ubuntu.iso'] },
  { category: 'junk', count: 4284, total_size: 6.8 * gib, risk: 'safe', description: 'Temporary and junk files', paths_sample: ['C:\\Users\\Alex\\AppData\\Local\\Temp', 'C:\\Windows\\Temp'] },
  { category: 'user_cache', count: 9823, total_size: 4.2 * gib, risk: 'safe', description: 'Browser and app caches', paths_sample: ['C:\\Users\\Alex\\AppData\\Local\\Google\\Chrome', 'C:\\Users\\Alex\\AppData\\Local\\Microsoft\\Edge'] },
  { category: 'old_log', count: 317, total_size: 1.3 * gib, risk: 'safe', description: 'Old log files', paths_sample: ['C:\\Users\\Alex\\AppData\\Local\\Temp\\update.log'] }
];

const progress = [
  { files: 18422, dirs: 840, bytes: 3.7 * gib, rate_fps: 28340, elapsed_s: 0.8, remain_s: 7.5, cached: 10820, current: 'Indexing the C: drive…', percent: 12, estimated: true, errors: 0, recent: ['C:\\Users\\Alex\\Documents\\brief.docx'] },
  { files: 79308, dirs: 2931, bytes: 18.2 * gib, rate_fps: 51750, elapsed_s: 1.7, remain_s: 4.9, cached: 32640, current: 'Scanning C:\\Users\\Alex\\Videos', percent: 38, estimated: true, errors: 0, recent: ['C:\\Users\\Alex\\Videos\\archive_2024.mkv'] },
  { files: 164802, dirs: 6240, bytes: 49.4 * gib, rate_fps: 63420, elapsed_s: 2.6, remain_s: 2.8, cached: 51008, current: 'Scanning C:\\Users\\Alex\\AppData', percent: 68, estimated: true, errors: 2, recent: ['C:\\Users\\Alex\\AppData\\Local\\Temp\\render-cache.tmp'] },
  { files: 226194, dirs: 8911, bytes: 71.6 * gib, rate_fps: 58910, elapsed_s: 3.5, remain_s: 1.1, cached: 67201, current: 'Scanning C:\\ProgramData', percent: 91, estimated: true, errors: 3, recent: ['C:\\ProgramData\\Package Cache\\installer.msi'] },
  { files: 248631, dirs: 9770, bytes: 82.9 * gib, rate_fps: 0, elapsed_s: 4.4, remain_s: 0.4, cached: 70118, current: 'Analyzing large files and safety rules…', percent: 99, estimated: true, errors: 3, recent: [] },
  { files: 248631, dirs: 9770, bytes: 82.9 * gib, rate_fps: 0, elapsed_s: 4.8, remain_s: 0, cached: 70118, current: 'Results are ready', percent: 100, estimated: false, errors: 3, recent: [] }
];

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
  let progressIndex = 0;

  await page.route('https://app.local/**', async route => {
    const url = new URL(route.request().url());
    const pathname = url.pathname;
    const json = value => route.fulfill({ status: 200, contentType: 'application/json; charset=utf-8', body: JSON.stringify(value) });

    if (pathname === '/' || pathname === '/index.html') {
      return route.fulfill({ status: 200, contentType: 'text/html; charset=utf-8', body: fs.readFileSync(path.join(root, 'web', 'index.html')) });
    }
    if (pathname === '/style.css') {
      return route.fulfill({ status: 200, contentType: 'text/css; charset=utf-8', body: fs.readFileSync(path.join(root, 'web', 'style.css')) });
    }
    if (pathname === '/app.js' || pathname === '/i18n.js') {
      return route.fulfill({ status: 200, contentType: 'text/javascript; charset=utf-8', body: fs.readFileSync(path.join(root, 'web', pathname.slice(1))) });
    }
    if (pathname === '/api/config') {
      if (route.request().method() === 'PUT') Object.assign(config, route.request().postDataJSON());
      return json(config);
    }
    if (pathname === '/api/smart-scan') {
      progressIndex = 0;
      return json({ scan_id: 27, status: 'started', categories: [], total_reclaimable: 0, total_files: 0 });
    }
    if (pathname === '/api/progress') {
      const current = progress[Math.min(progressIndex, progress.length - 1)];
      const done = progressIndex >= progress.length - 1;
      progressIndex += 1;
      return json({ progress: current, done });
    }
    if (pathname === '/api/smart-categories') {
      return json({ categories, total_reclaimable: 12.3 * gib, total_files: 14457, scan_id: 27, status: 'complete' });
    }
    if (pathname === '/api/results') return json({ items: findings, total: findings.length });
    if (pathname === '/api/stop') return json({ success: true });
    if (pathname === '/api/smart-clean') return json({ deleted: 0, failed: 0, total_bytes: 0, errors: [], success: true });
    return route.fulfill({ status: 404, contentType: 'text/plain', body: 'Not found' });
  });

  await page.goto('https://app.local/', { waitUntil: 'networkidle' });
  await page.evaluate(() => localStorage.removeItem('unused-removal-language'));
  await page.reload({ waitUntil: 'networkidle' });
  await page.waitForSelector('[data-language="en"].active');

  // Functional localization check: both directions must update real UI text.
  await page.click('[data-language="ru"]');
  await page.waitForFunction(() => document.querySelector('.smart-scan-hero h3')?.textContent.includes('Освободите место'));
  await page.click('[data-language="en"]');
  await page.waitForFunction(() => document.querySelector('.smart-scan-hero h3')?.textContent.includes('See what is taking'));

  await page.screenshot({ path: path.join(output, '01-home.png') });
  await page.click('.safety-card[data-level="balanced"]');
  await page.waitForFunction(() => document.querySelector('#smart-progress-percent')?.textContent.includes('68'));
  await page.locator('#smart-scan-progress').scrollIntoViewIfNeeded();
  await page.screenshot({ path: path.join(output, '02-progress.png') });
  await page.waitForSelector('#smart-results-phase:not(.hidden)', { timeout: 10000 });
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(output, '03-results.png') });
  await page.click('#btn-smart-review');
  await page.waitForSelector('#results-phase:not(.hidden)');
  await page.waitForTimeout(250);
  await page.screenshot({ path: path.join(output, '04-details.png') });

  await browser.close();
  process.stdout.write(`Rendered ${output}\n`);
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
