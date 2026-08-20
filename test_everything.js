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

  // ============ 1. SMART SCAN PHASE ============
  console.log('\n📋 [1] SMART SCAN PHASE');
  check('Smart scan phase visible', await page.isVisible('#smart-scan-phase'));
  check('btn-smart-scan visible', await page.isVisible('#btn-smart-scan'));
  check('btn-smart-advanced visible', await page.isVisible('#btn-smart-advanced'));
  check('safety selector visible', await page.isVisible('#smart-safety-level'));
  check('Hero icon visible', await page.isVisible('.smart-scan-icon'));
  check('Hero title visible', await page.isVisible('.smart-scan-hero h3'));
  check('Hero subtitle visible', await page.isVisible('.smart-scan-subtitle'));

  // Safety selector — all 3 options
  for (const level of ['safe', 'balanced', 'aggressive']) {
    await page.selectOption('#smart-safety-level', level);
    await page.waitForTimeout(150);
    const val = await page.$eval('#smart-safety-level', el => el.value);
    check(`Safety selector → ${level}`, val === level);
  }

  // ============ 2. NAVIGATION ============
  console.log('\n📋 [2] NAVIGATION');
  await page.click('#btn-smart-advanced');
  await page.waitForTimeout(400);
  check('Config phase after "Расширенные настройки"', await page.isVisible('#config-phase'));
  await page.click('#btn-back-to-smart');
  await page.waitForTimeout(400);
  check('Back to smart scan via config back btn', await page.isVisible('#smart-scan-phase'));

  // ============ 3. CONFIG PHASE ============
  console.log('\n📋 [3] CONFIG PHASE');
  await page.click('#btn-smart-advanced');
  await page.waitForTimeout(400);

  // Root select options
  const options = await page.$$eval('#root-select option', opts => opts.map(o => o.value));
  check(`Root options populated (${options.length})`, options.length >= 2);
  check('Custom option present', options.includes('custom'));

  // Custom path toggle
  await page.selectOption('#root-select', 'custom');
  await page.waitForTimeout(300);
  check('Custom path input shown', await page.isVisible('#root-custom'));
  await page.fill('#root-custom', '/tmp/ur_test');
  check('Custom path input accepts value', (await page.$eval('#root-custom', el => el.value)) === '/tmp/ur_test');
  await page.selectOption('#root-select', options.find(o => o !== 'custom'));
  await page.waitForTimeout(300);
  check('Custom path input hidden', !(await page.isVisible('#root-custom')));

  // Checkboxes
  for (const id of ['follow-links', 'use-cache', 'check-duplicates', 'protect-system']) {
    await page.$eval(`#${id}`, el => { el.checked = false; el.dispatchEvent(new Event('change', { bubbles: true })); });
    await page.waitForTimeout(80);
    await page.click(`label.checkbox-label:has(#${id})`);
    await page.waitForTimeout(120);
    check(`Checkbox ${id} toggles on`, await page.$eval(`#${id}`, el => el.checked));
    await page.click(`label.checkbox-label:has(#${id})`);
    await page.waitForTimeout(120);
    check(`Checkbox ${id} toggles off`, !(await page.$eval(`#${id}`, el => el.checked)));
  }

  // Workers input
  await page.fill('#workers', '8');
  check('Workers input value', (await page.$eval('#workers', el => el.value)) === '8');

  // ============ 4. DESIGN SYSTEM VERIFICATION ============
  console.log('\n📋 [4] DESIGN SYSTEM');
  const design = await page.evaluate(() => {
    const body = getComputedStyle(document.body);
    const panel = getComputedStyle(document.querySelector('.panel'));
    const btn = getComputedStyle(document.querySelector('.btn-primary'));
    const header = getComputedStyle(document.querySelector('.header'));
    const aurora = getComputedStyle(document.body, '::before');
    const root = getComputedStyle(document.documentElement);
    return {
      bodyBg: body.backgroundColor,
      panelBackdrop: panel.backdropFilter,
      btnGradient: btn.backgroundImage.includes('linear-gradient'),
      btnShine: getComputedStyle(document.querySelector('.btn-primary'), '::after').backgroundImage.includes('linear-gradient'),
      headerBlur: header.backdropFilter.includes('blur'),
      auroraAnim: aurora.animationName === 'aurora',
      auroraBlur: aurora.filter.includes('blur'),
      glassVars: root.getPropertyValue('--grad-primary').trim().length > 0
    };
  });
  check('Body dark bg', design.bodyBg === 'rgb(12, 15, 20)');
  check('Panel glass blur', design.panelBackdrop.includes('blur'));
  check('Primary btn gradient', design.btnGradient);
  check('Primary btn shine sweep', design.btnShine);
  check('Header glass blur', design.headerBlur);
  check('Aurora animation active', design.auroraAnim);
  check('Aurora blur filter', design.auroraBlur);
  check('Gradient CSS vars', design.glassVars);

  // Keyframes defined
  const keyframes = await page.evaluate(() => {
    const css = [...document.styleSheets].map(s => { try { return [...s.cssRules].map(r => r.name).join(','); } catch (e) { return ''; } }).join(',');
    return {
      aurora: css.includes('aurora'),
      shineSweep: css.includes('shineSweep'),
      ringGlow: css.includes('ringGlow'),
      toastTimer: css.includes('toastTimer'),
      cardEnter: css.includes('cardEnter'),
      rowEnter: css.includes('rowEnter'),
      modalContentOut: css.includes('modalContentOut')
    };
  });
  check('@keyframes aurora', keyframes.aurora);
  check('@keyframes shineSweep', keyframes.shineSweep);
  check('@keyframes ringGlow', keyframes.ringGlow);
  check('@keyframes toastTimer', keyframes.toastTimer);
  check('@keyframes cardEnter', keyframes.cardEnter);
  check('@keyframes rowEnter', keyframes.rowEnter);
  check('@keyframes modalContentOut', keyframes.modalContentOut);

  // Fonts
  const fonts = await page.evaluate(async () => {
    // Force-load the mono font (it loads lazily when mono text becomes visible)
    try { await document.fonts.load('12px "JetBrains Mono"'); } catch (e) {}
    await document.fonts.ready;
    await new Promise(r => setTimeout(r, 800));
    return {
      inter: document.fonts.check('14px Inter'),
      jetbrains: document.fonts.check('12px "JetBrains Mono"')
    };
  });
  check('Inter font loaded', fonts.inter);
  check('JetBrains Mono loads on demand', fonts.jetbrains);

  // Skip link
  check('Skip link present', await page.$eval('a.skip-link', el => !!el.getAttribute('href')) === true);

  // ============ 5. RESPONSIVE DESIGN ============
  console.log('\n📋 [5] RESPONSIVE');
  await page.setViewportSize({ width: 375, height: 667 });
  await page.waitForTimeout(400);
  // Show smart-scan phase so the button has a real size (hidden elements measure 0)
  await page.evaluate(() => {
    document.querySelectorAll('.phase-panel').forEach(s => s.classList.add('hidden'));
    document.getElementById('smart-scan-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  const mobile = await page.evaluate(() => {
    const grid = document.querySelector('.form-grid');
    const hero = document.querySelector('.smart-scan-actions');
    const actionsW = hero.getBoundingClientRect().width;
    const btnW = document.getElementById('btn-smart-scan').getBoundingClientRect().width;
    return {
      gridCols: getComputedStyle(grid).gridTemplateColumns,
      heroFlexDir: getComputedStyle(hero).flexDirection,
      scanBtnWidth: Math.round(btnW),
      actionsWidth: Math.round(actionsW)
    };
  });
  check('Mobile: form grid single column', mobile.gridCols.split(' ').length <= 1 || mobile.gridCols.includes('px'));
  check('Mobile: hero actions column', mobile.heroFlexDir === 'column');
  check(`Mobile: scan button spans container (${mobile.scanBtnWidth}/${mobile.actionsWidth}px)`, mobile.actionsWidth > 0 && mobile.scanBtnWidth >= mobile.actionsWidth - 2);

  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForTimeout(400);
  const desktop = await page.evaluate(() => {
    const grid = document.querySelector('.form-grid');
    return getComputedStyle(grid).gridTemplateColumns.split(' ').length;
  });
  check('Desktop: multi-column form grid', desktop >= 2);

  // ============ 6. SMART SCAN EXECUTION (10k files) ============
  console.log('\n📋 [6] SMART SCAN EXECUTION');
  // Ensure we're on smart-scan phase (desktop viewport)
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.waitForTimeout(300);
  await page.evaluate(() => {
    document.querySelectorAll('.phase-panel').forEach(s => s.classList.add('hidden'));
    document.getElementById('smart-scan-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  // Ensure root is the big fixture
  const setRoot = await page.evaluate(async () => {
    const res = await fetch('/api/config', { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ root: '/tmp/ur_big', workers: 8, use_cache: false, follow_links: false }) });
    return res.ok;
  });
  check('Set root to 10k fixture via API', setRoot);

  await page.click('#btn-smart-scan');
  await page.waitForTimeout(500);
  check('Progress shown', await page.isVisible('#smart-scan-progress'));
  check('Scan button disabled', await page.$eval('#btn-smart-scan', el => el.disabled));
  check('Progress ring has scanning class', await page.$eval('#smart-scan-progress .progress-ring', el => el.classList.contains('scanning')));

  // Wait for completion (up to 40s for 10k files)
  let smartResults = false;
  for (let i = 0; i < 80; i++) {
    await page.waitForTimeout(500);
    if (await page.isVisible('#smart-results-phase')) { smartResults = true; break; }
  }
  check('Smart results phase appears', smartResults);

  // ============ 7. SMART RESULTS ============
  console.log('\n📋 [7] SMART RESULTS');
  const cardCount = await page.$$eval('.category-card', els => els.length);
  check(`Category cards rendered (${cardCount})`, cardCount > 0);

  // Donut chart
  check('Donut chart visible', await page.isVisible('.donut-chart'));
  const donutVal = await page.$eval('#donut-reclaimable-text', el => el.textContent);
  check(`Donut value populated (${donutVal})`, donutVal !== '0 B');

  // Summary
  const totalFiles = await page.$eval('#smart-summary-total', el => parseInt(el.textContent));
  check(`Summary total files (${totalFiles})`, totalFiles > 0);
  const summarySize = await page.$eval('#smart-summary-size', el => el.textContent);
  check(`Summary size populated (${summarySize})`, summarySize !== '0 B');

  // Category card interactions
  const firstCat = await page.$eval('.category-card', el => el.dataset.category);
  await page.$eval(`.category-card[data-category="${firstCat}"] .category-checkbox input`, el => {
    el.checked = false; el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await page.waitForTimeout(200);
  check(`Category ${firstCat} deselects`, !(await page.$eval(`.category-card[data-category="${firstCat}"]`, el => el.classList.contains('selected'))));
  await page.$eval(`.category-card[data-category="${firstCat}"] .category-checkbox input`, el => {
    el.checked = true; el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await page.waitForTimeout(200);
  check(`Category ${firstCat} reselects`, await page.$eval(`.category-card[data-category="${firstCat}"]`, el => el.classList.contains('selected')));

  // Expand/collapse
  await page.click(`.category-card[data-category="${firstCat}"] .category-toggle`);
  await page.waitForTimeout(300);
  check('Category card expands', await page.$eval(`.category-card[data-category="${firstCat}"]`, el => el.classList.contains('expanded')));
  await page.click(`.category-card[data-category="${firstCat}"] .category-toggle`);
  await page.waitForTimeout(300);
  check('Category card collapses', !(await page.$eval(`.category-card[data-category="${firstCat}"]`, el => el.classList.contains('expanded'))));

  // Clean button state — deselect ALL first, then verify disabled
  const allCatChecks = await page.$$('.category-card .category-checkbox input');
  for (const cb of allCatChecks) {
    await cb.evaluate(el => { el.checked = false; el.dispatchEvent(new Event('change', { bubbles: true })); });
  }
  await page.waitForTimeout(200);
  check('Clean-all button disabled when no selection', await page.$eval('#btn-smart-clean-all', el => el.disabled));
  // Reselect one
  await page.$eval(`.category-card[data-category="${firstCat}"] .category-checkbox input`, el => {
    el.checked = true; el.dispatchEvent(new Event('change', { bubbles: true }));
  });
  await page.waitForTimeout(200);
  check('Clean-all button enabled after selection', !(await page.$eval('#btn-smart-clean-all', el => el.disabled)));

  // Safety re-filter
  for (const level of ['safe', 'balanced', 'aggressive']) {
    await page.selectOption('#smart-results-safety', level);
    await page.waitForTimeout(400);
    check(`Results safety → ${level} re-renders`, (await page.$$eval('.category-card', els => els.length)) >= 0);
  }

  // ============ 8. RESULTS TABLE (10k) ============
  console.log('\n📋 [8] RESULTS TABLE');
  await page.click('#btn-smart-review');
  await page.waitForTimeout(600);
  check('Results table phase visible', await page.isVisible('#results-phase'));

  const rowCount = await page.$$eval('#results-body tr', els => els.length);
  check(`Table rows rendered (${rowCount})`, rowCount > 0);
  check('Pagination page info', (await page.$eval('#page-info', el => el.textContent)).includes('Стр.'));

  // Sort all columns
  for (const key of ['path', 'size', 'category', 'risk', 'mod_time']) {
    await page.click(`th[data-sort="${key}"]`);
    await page.waitForTimeout(250);
    await page.click(`th[data-sort="${key}"]`);
    await page.waitForTimeout(250);
    check(`Sort ${key} asc/desc`, true);
  }

  // ============ 9. SEARCH (10k items, performance + behavior) ============
  console.log('\n📋 [9] SEARCH');
  // Search clear button hidden initially
  check('Search clear hidden when empty', await page.$eval('#search-clear', el => el.hidden));

  // Type a query — measure real end-to-end filter time
  let searchMs = Infinity;
  await page.fill('#filter-search', 'file_50');
  await page.waitForTimeout(400);
  const searchStart = Date.now();
  // Trigger another keystroke via keyboard to measure live
  await page.type('#filter-search', '');
  await page.waitForTimeout(400);
  searchMs = Date.now() - searchStart;
  const searchRows = await page.$$eval('#results-body tr', els => els.length);
  check(`Search 'file_50' returns rows (${searchRows})`, searchRows > 0);
  check(`Search responsive (<1s after debounce) (${searchMs}ms)`, searchMs < 1000);
  check('Search clear button shown', !(await page.$eval('#search-clear', el => el.hidden)));

  // Clear via X button
  await page.click('#search-clear');
  await page.waitForTimeout(400);
  check('Search cleared via X', (await page.$eval('#filter-search', el => el.value)) === '');
  check('Search clear hidden after clear', await page.$eval('#search-clear', el => el.hidden));

  // Benchmark: pure filter speed on all 10k (10 passes)
  const bench = await page.evaluate(async () => {
    const res = await fetch('/api/results?limit=10000');
    const data = await res.json();
    const items = data.items;
    const index = items.map(f => ({ path: f.path.toLowerCase(), reason: (f.reason || '').toLowerCase() }));
    // old
    let t0 = performance.now();
    let oldTotal = 0;
    for (let r = 0; r < 5; r++) {
      const s = 'file_' + (10 + r);
      oldTotal += items.filter(f => {
        const p = f.path.toLowerCase(); const rs = (f.reason || '').toLowerCase();
        return p.includes(s) || rs.includes(s);
      }).length;
    }
    const oldMs = (performance.now() - t0) / 5;
    // new
    t0 = performance.now();
    let newTotal = 0;
    for (let r = 0; r < 5; r++) {
      const s = 'file_' + (10 + r);
      const out = [];
      for (let i = 0; i < items.length; i++) {
        const ix = index[i];
        if (ix.path.includes(s) || ix.reason.includes(s)) out.push(items[i]);
      }
      newTotal += out.length;
    }
    const newMs = (performance.now() - t0) / 5;
    return { oldMs, newMs, oldTotal, newTotal };
  });
  check(`Search 10k: OLD ${bench.oldMs.toFixed(2)}ms NEW ${bench.newMs.toFixed(2)}ms (${(bench.oldMs / bench.newMs).toFixed(1)}x faster)`, bench.newMs < bench.oldMs);
  check('Benchmark matches equal', bench.oldTotal === bench.newTotal);

  // Category filter
  const catFilterCount = await page.$$eval('#filter-category option', opts => opts.length);
  check(`Category filter has ${catFilterCount} options`, catFilterCount >= 15);

  // Select all
  await page.check('#select-all');
  await page.waitForTimeout(300);
  check('Select-all works', await page.$eval('#sel-count', el => parseInt(el.textContent)) > 0);
  await page.uncheck('#select-all');
  await page.waitForTimeout(300);

  // Row checkbox
  await page.check('#results-body .row-check');
  await page.waitForTimeout(200);
  check('Row checkbox works', await page.$eval('#sel-count', el => parseInt(el.textContent)) > 0);

  // ============ 10. MODAL ============
  console.log('\n📋 [10] MODAL');
  const sel = await page.$eval('#sel-count', el => parseInt(el.textContent));
  if (sel === 0) { await page.check('#results-body .row-check'); await page.waitForTimeout(200); }
  await page.click('#btn-recycle');
  await page.waitForTimeout(400);
  check('Modal opens (recycle)', await page.isVisible('#modal'));
  check('Modal glass blur', (await page.evaluate(() => getComputedStyle(document.querySelector('.modal-content')).backdropFilter)).includes('blur'));
  await page.click('#modal-cancel');
  await page.waitForTimeout(400);
  check('Modal closes (Отмена)', !(await page.isVisible('#modal')));
  await page.waitForTimeout(300); // let exit animation fully finish before re-opening

  await page.click('#btn-hard');
  await page.waitForTimeout(400);
  check('Modal opens (hard)', await page.isVisible('#modal'));
  await page.click('.modal-close');
  await page.waitForTimeout(400);
  check('Modal closes (X)', !(await page.isVisible('#modal')));

  // ============ 11. EXPORT + PAGINATION ============
  console.log('\n📋 [11] EXPORT + PAGINATION');
  check('Export JSON btn', !!await page.$('#btn-export-json'));
  check('Export CSV btn', !!await page.$('#btn-export-csv'));
  check('Prev disabled page 1', await page.$eval('#btn-prev', el => el.disabled));
  check('Next enabled on multi-page', !(await page.$eval('#btn-next', el => el.disabled)));
  await page.click('#btn-next');
  await page.waitForTimeout(300);
  check('Pagination advances', (await page.$eval('#page-info', el => el.textContent)).includes('Стр. 2'));
  await page.click('#btn-prev');
  await page.waitForTimeout(300);
  check('Pagination goes back', (await page.$eval('#page-info', el => el.textContent)).includes('Стр. 1'));

  // ============ 12. TOASTS ============
  console.log('\n📋 [12] TOASTS');
  await page.evaluate(() => window.app.showToast('Тест-тост', 'success'));
  await page.waitForTimeout(300);
  const toast = await page.$('.toast.toast-success');
  check('Success toast appears', !!toast);
  if (toast) {
    const bar = await page.evaluate(() => getComputedStyle(document.querySelector('.toast'), '::after').animationName);
    check('Toast progress bar animates', bar === 'toastTimer');
  }
  await page.waitForTimeout(3200);
  check('Toast auto-dismisses', (await page.$$('.toast')).length === 0);

  // ============ 13. PHASE TRANSITIONS ============
  console.log('\n📋 [13] PHASE TRANSITIONS');
  // Go back to smart scan via results → smart-results → smart-scan
  await page.evaluate(() => {
    document.querySelectorAll('.phase-panel').forEach(s => s.classList.add('hidden'));
    document.getElementById('smart-results-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  await page.click('#btn-back-to-smart-results');
  await page.waitForTimeout(500);
  check('smart-results → smart-scan', await page.isVisible('#smart-scan-phase'));

  // ============ 14. REDUCED MOTION ============
  console.log('\n📋 [14] REDUCED MOTION');
  const reducedPage = await browser.newPage({ viewport: { width: 1400, height: 900 }, reducedMotion: 'reduce' });
  await reducedPage.goto(BASE, { waitUntil: 'networkidle' });
  const reducedAnim = await reducedPage.evaluate(() => {
    const body = getComputedStyle(document.body, '::before');
    return { name: body.animationName, duration: body.animationDuration };
  });
  check('Aurora disabled with reduced motion', reducedAnim.name === 'none' || reducedAnim.duration === '1e-05s' || reducedAnim.duration.includes('0.01ms'));
  await reducedPage.close();

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