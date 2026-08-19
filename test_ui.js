const { chromium } = require('playwright');

(async () => {
  const browser = await chromium.launch({ headless: false, slowMo: 100 });
  const page = await browser.newPage();
  
  // Capture console errors
  const errors = [];
  page.on('console', msg => {
    if (msg.type() === 'error') errors.push(msg.text());
  });
  page.on('pageerror', err => errors.push(err.message));

  console.log('🌐 Navigating to http://127.0.0.1:3081...');
  await page.goto('http://127.0.0.1:3081', { waitUntil: 'networkidle' });
  
  console.log('✅ Page loaded');
  
  // Test 1: Check initial phase (config)
  console.log('\n📋 Test 1: Config phase visible');
  const configPhase = await page.$('#config-phase');
  const isConfigVisible = await configPhase.isVisible();
  console.log(`   Config phase visible: ${isConfigVisible}`);
  
  // Test 2: Check all form elements
  console.log('\n📋 Test 2: Form elements present');
  const rootSelect = await page.$('#root-select');
  const workers = await page.$('#workers');
  const followLinks = await page.$('#follow-links');
  const useCache = await page.$('#use-cache');
  const checkDuplicates = await page.$('#check-duplicates');
  const protectSystem = await page.$('#protect-system');
  console.log(`   Root select: ${!!rootSelect}`);
  console.log(`   Workers input: ${!!workers}`);
  console.log(`   Follow links: ${!!followLinks}`);
  console.log(`   Use cache: ${!!useCache}`);
  console.log(`   Check duplicates: ${!!checkDuplicates}`);
  console.log(`   Protect system: ${!!protectSystem}`);
  
  // Test 3: Check checkbox animations (click label)
  console.log('\n📋 Test 3: Checkbox interaction');
  const followLinksLabel = await page.$('#follow-links + .checkmark');
  // Click the label parent
  await page.click('.checkbox-label:has(#follow-links)');
  const isChecked = await followLinks.isChecked();
  console.log(`   Follow links checked: ${isChecked}`);
  
  // Test 4: Custom path toggle
  console.log('\n📋 Test 4: Custom path toggle');
  await page.selectOption('#root-select', 'custom');
  const customInput = await page.$('#root-custom');
  const isCustomVisible = await customInput.isVisible();
  console.log(`   Custom input visible after select: ${isCustomVisible}`);
  
  // Test 5: Buttons present
  console.log('\n📋 Test 5: Buttons present');
  const btnScan = await page.$('#btn-scan');
  const btnStop = await page.$('#btn-stop');
  console.log(`   Scan button: ${!!btnScan}`);
  console.log(`   Stop button: ${!!btnStop}`);
  console.log(`   Scan button enabled: ${await btnScan.isEnabled()}`);
  console.log(`   Stop button disabled: ${!(await btnStop.isEnabled())}`);
  
  // Test 6: Click scan button (should show toast for missing root or start scan)
  console.log('\n📋 Test 6: Scan button click (no path selected)');
  await page.evaluate(() => {
    const select = document.getElementById('root-select');
    select.value = '';
    select.dispatchEvent(new Event('change'));
  });
  await btnScan.click();
  await page.waitForTimeout(500);
  const toasts = await page.$$('.toast');
  console.log(`   Toast shown: ${toasts.length > 0}`);
  if (toasts.length > 0) {
    const toastText = await toasts[0].textContent();
    console.log(`   Toast message: ${toastText}`);
  }
  
  // Test 7: Select a root path and try scan (will fail since no backend scan running, but UI should transition)
  console.log('\n📋 Test 7: Select root path');
  const options = await page.$$eval('#root-select option', opts => opts.map(o => o.value));
  console.log(`   Available options: ${options.join(', ')}`);
  if (options.length > 0 && options[0] !== 'custom') {
    await page.selectOption('#root-select', options[0]);
    console.log(`   Selected: ${options[0]}`);
  }
  
  // Test 8: Check CSS animations are working (check computed styles)
  console.log('\n📋 Test 8: CSS animations/transition present');
  const transitionCheck = await page.evaluate(() => {
    const panel = document.querySelector('.panel');
    const btn = document.querySelector('.btn-primary');
    const ring = document.querySelector('.progress-ring-fill');
    return {
      panelTransition: window.getComputedStyle(panel).transition,
      btnTransition: window.getComputedStyle(btn).transition,
      ringTransition: ring ? window.getComputedStyle(ring).transition : 'N/A'
    };
  });
  console.log(`   Panel transition: ${transitionCheck.panelTransition}`);
  console.log(`   Button transition: ${transitionCheck.btnTransition}`);
  console.log(`   Ring transition: ${transitionCheck.ringTransition}`);
  
  // Test 9: Check responsive design - viewport changes
  console.log('\n📋 Test 9: Responsive design');
  await page.setViewportSize({ width: 375, height: 667 }); // Mobile
  await page.waitForTimeout(300);
  const mobileLayout = await page.evaluate(() => {
    const grid = document.querySelector('.form-grid');
    const toolbar = document.querySelector('.results-toolbar');
    return {
      gridColumns: window.getComputedStyle(grid).gridTemplateColumns,
      toolbarFlexDir: window.getComputedStyle(toolbar).flexDirection
    };
  });
  console.log(`   Mobile grid columns: ${mobileLayout.gridColumns}`);
  console.log(`   Mobile toolbar direction: ${mobileLayout.toolbarFlexDir}`);
  
  await page.setViewportSize({ width: 1400, height: 900 }); // Desktop
  await page.waitForTimeout(300);
  const desktopLayout = await page.evaluate(() => {
    const grid = document.querySelector('.form-grid');
    return { gridColumns: window.getComputedStyle(grid).gridTemplateColumns };
  });
  console.log(`   Desktop grid columns: ${desktopLayout.gridColumns}`);
  
  // Test 10: Check modal opens
  console.log('\n📋 Test 10: Modal interaction');
  // Add a finding to state and try to open modal
  await page.evaluate(() => {
    window.app.state = window.app.state || {};
    window.app.state.selectedPaths = new Set(['/test/file.txt']);
    window.app.state.findings = [{ path: '/test/file.txt', size: 1024, category: 'junk', reason: 'test', risk: 'safe', mod_time: new Date().toISOString() }];
    window.app.state.filteredFindings = window.app.state.findings;
    window.app.confirmDelete('recycle');
  });
  await page.waitForTimeout(300);
  const modal = await page.$('#modal');
  const isModalVisible = await modal.isVisible();
  console.log(`   Modal visible: ${isModalVisible}`);
  if (isModalVisible) {
    await page.click('#modal-cancel');
    await page.waitForTimeout(200);
    console.log(`   Modal closed after cancel`);
  }
  
  // Test 11: Phase transition via API
  console.log('\n📋 Test 11: Phase transition');
  await page.evaluate(() => {
    // Manually trigger phase transition to results
    const state = window.app.state || {};
    state.phase = 'results';
    state.findings = [{ path: '/test/file.txt', size: 1024, category: 'junk', reason: 'test', risk: 'safe', mod_time: new Date().toISOString() }];
    state.filteredFindings = state.findings;
    // Call renderTable manually
    if (window.app.renderTable) window.app.renderTable();
    // Hide config, show results
    document.getElementById('config-phase').classList.add('hidden');
    document.getElementById('results-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  const resultsPhase = await page.$('#results-phase');
  console.log(`   Results phase visible: ${await resultsPhase.isVisible()}`);
  
  // Test 12: Progress ring SVG exists
  console.log('\n📋 Test 12: Progress ring SVG');
  const ringSvg = await page.$('.progress-ring-svg');
  const ringFill = await page.$('#progress-ring-fill');
  console.log(`   Ring SVG: ${!!ringSvg}`);
  console.log(`   Ring fill: ${!!ringFill}`);
  if (ringFill) {
    const dashArray = await ringFill.getAttribute('stroke-dasharray');
    const dashOffset = await ringFill.getAttribute('stroke-dashoffset');
    console.log(`   Stroke dasharray: ${dashArray}`);
    console.log(`   Stroke dashoffset: ${dashOffset}`);
  }
  
  // Test 13: Check all CSS custom properties
  console.log('\n📋 Test 13: CSS custom properties');
  const cssVars = await page.evaluate(() => {
    const styles = getComputedStyle(document.documentElement);
    const vars = [
      '--bg', '--bg-elevated', '--bg-card', '--accent', '--accent-glow',
      '--radius', '--shadow', '--transition', '--fast', '--base', '--slow', '--spring'
    ];
    const result = {};
    vars.forEach(v => result[v] = styles.getPropertyValue(v).trim());
    return result;
  });
  console.log('   Key CSS variables:');
  Object.entries(cssVars).forEach(([k, v]) => console.log(`     ${k}: ${v}`));
  
  // Test 14: Check font loading
  console.log('\n📋 Test 14: Font loading');
  const fonts = await page.evaluate(() => {
    return {
      inter: document.fonts.check('14px Inter'),
      jetbrains: document.fonts.check('14px "JetBrains Mono"')
    };
  });
  console.log(`   Inter loaded: ${fonts.inter}`);
  console.log(`   JetBrains Mono loaded: ${fonts.jetbrains}`);
  
  // Summary
  console.log('\n' + '='.repeat(50));
  console.log('📊 TEST SUMMARY');
  console.log('='.repeat(50));
  console.log(`Console errors: ${errors.length}`);
  if (errors.length > 0) {
    errors.forEach(e => console.log(`   ❌ ${e}`));
  } else {
    console.log('   ✅ No console errors');
  }
  
  await browser.close();
  process.exit(errors.length > 0 ? 1 : 0);
})();