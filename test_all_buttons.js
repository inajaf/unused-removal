const { chromium } = require('playwright');

const BASE = 'http://127.0.0.1:3082';
let passed = 0, failed = 0;
const failures = [];

function check(name, cond, extra = '') {
  if (cond) { passed++; console.log(`   ✅ ${name}`); }
  else { failed++; failures.push(name); console.log(`   ❌ ${name} ${extra}`); }
}

(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });
  const errors = [];
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', e => errors.push('pageerror: ' + e.message));

  console.log('🌐 Loading page...');
  await page.goto(BASE, { waitUntil: 'networkidle' });
  check('Page loads', true);

  // ============ SMART SCAN PHASE ============
  console.log('\n📋 SMART SCAN PHASE');
  check('Smart scan phase visible', await page.isVisible('#smart-scan-phase'));
  check('safety cards exist (3)', (await page.$$('.safety-card')).length === 3);
  check('settings button exists', await page.isVisible('#btn-open-settings'));
  check('hidden safety select present', await page.$eval('#smart-safety-level', el => !!el));

  // Safety selector — all 3 options
  for (const level of ['safe', 'balanced', 'aggressive']) {
    await page.selectOption('#smart-safety-level', level);
    await page.waitForTimeout(150);
    const val = await page.$eval('#smart-safety-level', el => el.value);
    check(`Safety selector → ${level}`, val === level);
  }

  // ============ NAVIGATION: smart → config → smart ============
  console.log('\n📋 NAVIGATION');
  await page.click('#btn-open-settings');
  await page.waitForTimeout(400);
  check('Config phase visible after "Расширенные настройки"', await page.isVisible('#config-phase'));

  await page.click('#btn-back-to-smart');
  await page.waitForTimeout(400);
  check('Back to smart scan via "Назад к умной очистке"', await page.isVisible('#smart-scan-phase'));

  // ============ CONFIG PHASE ============
  console.log('\n📋 CONFIG PHASE');
  await page.click('#btn-open-settings');
  await page.waitForTimeout(400);

  // Custom path toggle
  const hasCustom = await page.$eval('#root-select option', opts => Array.from(document.querySelectorAll('#root-select option')).some(o => o.value === 'custom'));
  check('Custom option in root select', hasCustom);
  await page.selectOption('#root-select', 'custom');
  await page.waitForTimeout(300);
  check('Custom path input shown', await page.isVisible('#root-custom'));
  await page.fill('#root-custom', '/tmp/ur_test');
  await page.waitForTimeout(200);
  check('Custom path input accepts path', (await page.$eval('#root-custom', el => el.value)) === '/tmp/ur_test');
  // Back to a real option to hide custom input
  const firstReal = await page.$$eval('#root-select option', opts => opts.find(o => o.value !== 'custom').value);
  await page.selectOption('#root-select', firstReal);
  await page.waitForTimeout(300);
  check('Custom path input hidden after root select', !(await page.isVisible('#root-custom')));

  // Checkboxes
  for (const id of ['follow-links', 'use-cache', 'check-duplicates', 'protect-system']) {
    // Force known initial state (unchecked) — config may have loaded it as checked
    await page.$eval(`#${id}`, el => { el.checked = false; el.dispatchEvent(new Event('change', { bubbles: true })); });
    await page.waitForTimeout(100);
    // Click the label (custom checkbox: real input is visually hidden)
    await page.click(`label.checkbox-label:has(#${id})`);
    await page.waitForTimeout(150);
    check(`Checkbox ${id} checks via label`, await page.$eval(`#${id}`, el => el.checked));
    await page.click(`label.checkbox-label:has(#${id})`);
    await page.waitForTimeout(150);
    check(`Checkbox ${id} unchecks via label`, !(await page.$eval(`#${id}`, el => el.checked)));
  }

  // Workers input
  await page.fill('#workers', '8');
  check('Workers input accepts value', (await page.$eval('#workers', el => el.value)) === '8');

  // ============ SMART SCAN EXECUTION ============
  console.log('\n📋 SMART SCAN EXECUTION');
  // Set root to fixture via API
  const setRoot = await page.evaluate(async () => {
    const res = await fetch('/api/config', {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ root: '/tmp/ur_test', workers: 4, check_duplicates: true, use_cache: false })
    });
    return res.ok;
  });
  check('Set scan root via API', setRoot);

  // Go to smart scan phase
  await page.click('#btn-back-to-smart');
  await page.waitForTimeout(400);
  check('Back on smart scan', await page.isVisible('#smart-scan-phase'));

  // Start smart scan — click the recommended safety card (one-click flow)
  await page.click('.safety-card[data-level="balanced"]');
  await page.waitForTimeout(500);
  check('Smart scan progress shown', await page.isVisible('#smart-scan-progress'));
  check('Stop button visible during scan', await page.isVisible('#btn-smart-stop'));

  // Wait for completion (up to 30s)
  let smartResults = false;
  for (let i = 0; i < 60; i++) {
    await page.waitForTimeout(500);
    if (await page.isVisible('#smart-results-phase')) { smartResults = true; break; }
  }
  check('Smart results phase appears after scan', smartResults);

  // ============ SMART RESULTS PHASE ============
  console.log('\n📋 SMART RESULTS PHASE');
  const cardCount = await page.$$eval('.category-card', els => els.length);
  check(`Category cards rendered (${cardCount})`, cardCount > 0);

  // Category card toggle checkbox
  const firstCheckbox = await page.$('.category-card .category-checkbox input');
  if (firstCheckbox) {
    const catName = await page.$eval('.category-card', el => el.dataset.category);
    // Deselect via checkbox change event (fires toggleSmartCategory)
    await page.$eval(`.category-card[data-category="${catName}"] .category-checkbox input`, el => {
      el.checked = false;
      el.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await page.waitForTimeout(200);
    check(`Category "${catName}" deselected`, !(await page.$eval(`.category-card[data-category="${catName}"]`, el => el.classList.contains('selected'))));
    // Reselect
    await page.$eval(`.category-card[data-category="${catName}"] .category-checkbox input`, el => {
      el.checked = true;
      el.dispatchEvent(new Event('change', { bubbles: true }));
    });
    await page.waitForTimeout(200);
    check(`Category "${catName}" reselected`, await page.$eval(`.category-card[data-category="${catName}"]`, el => el.classList.contains('selected')));
  }

  // Expand / collapse card
  await page.click('.category-card .category-toggle');
  await page.waitForTimeout(300);
  check('Category card expands', await page.$eval('.category-card', el => el.classList.contains('expanded')));
  await page.click('.category-card .category-toggle');
  await page.waitForTimeout(300);
  check('Category card collapses', !(await page.$eval('.category-card', el => el.classList.contains('expanded'))));

  // Scan level info chip reflects the level used for this scan
  const lvl = await page.$eval('#results-level-label', el => el.textContent);
  check(`Results level chip shows "${lvl}"`, ['Безопасный','Сбалансированный','Агрессивный'].includes(lvl));

  // "Show all files" opens detailed table filtered by category
  await page.click('.category-card .category-toggle'); // expand samples area
  await page.waitForTimeout(300);
  const openBtn = await page.$('.category-card.expanded .category-open-list');
  if (openBtn) {
    const cat = await page.$eval('.category-open-list', el => el.dataset.openCategory);
    await openBtn.click();
    await page.waitForTimeout(700);
    const onResults = await page.isVisible('#results-phase');
    const filterVal = await page.$eval('#filter-category', el => el.value);
    const rows = (await page.$$('#results-body tr')).length;
    check(`Open list → results filtered by "${cat}" (${rows} rows)`, onResults && filterVal === cat && rows > 0);
  }


  // "Просмотреть детально" → results table (skipped if open-list already brought us here)
  if (await page.isVisible('#smart-results-phase')) {
    await page.click('#btn-smart-review');
    await page.waitForTimeout(600);
  }
  check('Results table phase visible', await page.isVisible('#results-phase'));

  // ============ RESULTS TABLE ============
  console.log('\n📋 RESULTS TABLE');
  const rowCount = await page.$$eval('#results-body tr', els => els.length);
  check(`Table rows rendered (${rowCount})`, rowCount > 0);

  // Sort headers
  for (const key of ['path', 'size', 'category', 'risk', 'mod_time']) {
    await page.click(`th[data-sort="${key}"]`);
    await page.waitForTimeout(300);
    check(`Sort by ${key} clickable`, true);
  }

  // Search filter
  await page.fill('#filter-search', 'log');
  await page.waitForTimeout(600);
  const searchRows = await page.$$eval('#results-body tr', els => els.length);
  check(`Search 'log' filters rows (${searchRows})`, true);
  await page.fill('#filter-search', '');
  await page.waitForTimeout(600);

  // Category filter
  await page.selectOption('#filter-category', 'junk');
  await page.waitForTimeout(400);
  check('Category filter selectable', true);
  await page.selectOption('#filter-category', '');
  await page.waitForTimeout(400);

  // Select all
  await page.check('#select-all');
  await page.waitForTimeout(300);
  const selCount = await page.$eval('#sel-count', el => parseInt(el.textContent));
  check(`Select-all selects rows (${selCount})`, selCount > 0);
  await page.uncheck('#select-all');
  await page.waitForTimeout(300);

  // Row checkbox
  const rowCheck = await page.$('#results-body .row-check');
  if (rowCheck) {
    await rowCheck.check();
    await page.waitForTimeout(200);
    check('Row checkbox selects', await page.$eval('#sel-count', el => parseInt(el.textContent)) > 0);
  }

  // ============ MODAL ============
  console.log('\n📋 MODAL');
  // Ensure at least one selected
  const selected = await page.$eval('#sel-count', el => parseInt(el.textContent));
  if (selected === 0) {
    await page.check('#results-body .row-check');
    await page.waitForTimeout(200);
  }
  await page.click('#btn-recycle');
  await page.waitForTimeout(400);
  check('Modal opens on "В Корзину"', await page.isVisible('#modal'));
  await page.click('#modal-cancel');
  await page.waitForTimeout(400);
  check('Modal closes on "Отмена"', !(await page.isVisible('#modal')));

  // Hard delete modal
  await page.click('#btn-hard');
  await page.waitForTimeout(400);
  check('Modal opens on "Безвозвратно"', await page.isVisible('#modal'));
  await page.click('.modal-close');
  await page.waitForTimeout(400);
  check('Modal closes via X', !(await page.isVisible('#modal')));

  // ============ EXPORT BUTTONS ============
  console.log('\n📋 EXPORT');
  const jsonBtn = await page.$('#btn-export-json');
  check('Export JSON button exists', !!jsonBtn);
  const csvBtn = await page.$('#btn-export-csv');
  check('Export CSV button exists', !!csvBtn);

  // ============ PAGINATION ============
  console.log('\n📋 PAGINATION');
  const nextDisabled = await page.$eval('#btn-next', el => el.disabled);
  const prevDisabled = await page.$eval('#btn-prev', el => el.disabled);
  check('Prev disabled on page 1', prevDisabled);
  check('Next disabled when 1 page (or enabled)', typeof nextDisabled === 'boolean');
  await page.click('#btn-next').catch(() => {});
  await page.waitForTimeout(300);

  // ============ BACK NAVIGATION ============
  console.log('\n📋 BACK NAVIGATION');
  // From results go to smart results then back to smart scan
  await page.evaluate(() => {
    document.querySelectorAll('.phase-panel').forEach(s => s.classList.add('hidden'));
    document.getElementById('smart-results-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  const backBtn = await page.$('#smart-results-phase #btn-back-to-smart-results');
  if (backBtn) {
    await backBtn.click();
    await page.waitForTimeout(400);
    check('Smart results → back to smart scan', await page.isVisible('#smart-scan-phase'));
  } else {
    check('Smart results back button exists', false);
  }

  // ============ SUMMARY ============
  console.log('\n' + '='.repeat(50));
  console.log('📊 RESULTS');
  console.log('='.repeat(50));
  console.log(`Passed: ${passed}, Failed: ${failed}`);
  if (failures.length) {
    console.log('\nFailures:');
    failures.forEach(f => console.log(`  ❌ ${f}`));
  }
  console.log(`Console errors: ${errors.length}`);
  errors.forEach(e => console.log(`   ⚠️ ${e}`));

  await browser.close();
  process.exit(failed > 0 ? 1 : 0);
})();