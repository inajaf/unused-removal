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
  
  // Test 1: Check initial phase (smart-scan)
  console.log('\n📋 Test 1: Smart Scan phase visible');
  const smartScanPhase = await page.$('#smart-scan-phase');
  const isSmartScanVisible = await smartScanPhase.isVisible();
  console.log(`   Smart Scan phase visible: ${isSmartScanVisible}`);
  
  // Test 2: Config phase accessible
  console.log('\n📋 Test 2: Config phase accessible');
  await page.evaluate(() => document.getElementById('btn-smart-advanced').click());
  await page.waitForTimeout(300);
  const configPhase = await page.$('#config-phase');
  const isConfigVisible = await configPhase.isVisible();
  console.log(`   Config phase visible: ${isConfigVisible}`);
  
  // Go back to smart scan
  await page.evaluate(() => document.getElementById('btn-scan').click());
  await page.waitForTimeout(300);
  
  // Test 3: Check all form elements (navigate to config first)
  console.log('\n📋 Test 3: Config phase form elements');
  await page.evaluate(() => document.getElementById('btn-smart-advanced').click());
  await page.waitForTimeout(300);
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
  
  // Test 4: Check checkbox animations (click label)
  console.log('\n📋 Test 4: Checkbox interaction');
  await page.evaluate(() => {
    const checkbox = document.getElementById('follow-links');
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change', { bubbles: true }));
  });
  const isChecked = await followLinks.isChecked();
  console.log(`   Follow links checked: ${isChecked}`);
  
  // Go back to smart scan
  await page.evaluate(() => document.getElementById('btn-scan').click());
  await page.waitForTimeout(300);
  
  // Test 6: Buttons present
  console.log('\n📋 Test 6: Smart Scan buttons present');
  const btnSmartScan = await page.$('#btn-smart-scan');
  const btnSmartAdvanced = await page.$('#btn-smart-advanced');
  console.log(`   Smart Scan button: ${!!btnSmartScan}`);
  console.log(`   Advanced button: ${!!btnSmartAdvanced}`);
  console.log(`   Smart Scan button enabled: ${await btnSmartScan.isEnabled()}`);
  
  // Test 7: Smart safety selector
  console.log('\n📋 Test 7: Safety level selector');
  const safetySelect = await page.$('#smart-safety-level');
  console.log(`   Safety selector: ${!!safetySelect}`);
  await page.evaluate(() => {
    const sel = document.getElementById('smart-safety-level');
    if (sel) {
      sel.value = 'safe';
      sel.dispatchEvent(new Event('change'));
    }
  });
  await page.waitForTimeout(100);
  console.log(`   Safety level set to 'safe'`);
  await page.evaluate(() => {
    const sel = document.getElementById('smart-safety-level');
    sel.value = 'aggressive';
    sel.dispatchEvent(new Event('change'));
  });
  await page.waitForTimeout(100);
  console.log(`   Safety level set to 'aggressive'`);
  await page.evaluate(() => {
    const sel = document.getElementById('smart-safety-level');
    sel.value = 'balanced';
    sel.dispatchEvent(new Event('change'));
  });
  
  // Test 8: Smart scan button click (should show toast for missing root or start scan)
  console.log('\n📋 Test 8: Smart scan button click (no path selected)');
  await page.evaluate(() => {
    const select = document.getElementById('root-select');
    select.value = '';
    select.dispatchEvent(new Event('change'));
  });
  await page.evaluate(() => document.getElementById('btn-smart-scan').click());
  await page.waitForTimeout(500);
  const toasts = await page.$$('.toast');
  console.log(`   Toast shown: ${toasts.length > 0}`);
  if (toasts.length > 0) {
    const toastText = await toasts[0].textContent();
    console.log(`   Toast message: ${toastText}`);
  }
  
  // Test 9: Select a root path
  console.log('\n📋 Test 9: Select root path');
  await page.evaluate(() => document.getElementById('btn-smart-advanced').click());
  await page.waitForTimeout(300);
  const options = await page.$$eval('#root-select option', opts => opts.map(o => o.value));
  console.log(`   Available options: ${options.join(', ')}`);
  if (options.length > 0 && options[0] !== 'custom') {
    await page.selectOption('#root-select', options[0]);
    console.log(`   Selected: ${options[0]}`);
  }
  await page.evaluate(() => document.getElementById('btn-scan').click());
  await page.waitForTimeout(300);
  
  // Test 8: Check CSS animations are working (check computed styles)
  console.log('\n📋 Test 8: CSS animations/transition present');
  await page.evaluate(() => document.getElementById('btn-smart-advanced').click());
  await page.waitForTimeout(300);
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
  
  // Go back to smart scan
  await page.evaluate(() => document.getElementById('btn-scan').click());
  await page.waitForTimeout(300);
  
  // Test 10: Modal interaction
  console.log('\n📋 Test 10: Modal interaction');
  // First navigate to results phase
  await page.evaluate(() => {
    document.getElementById('config-phase').classList.add('hidden');
    document.getElementById('results-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  
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
    await page.evaluate(() => document.getElementById('modal-cancel').click());
    await page.waitForTimeout(200);
    console.log(`   Modal closed after cancel`);
  }
  
  // Go back to smart scan
  await page.evaluate(() => {
    document.getElementById('results-phase').classList.add('hidden');
    document.getElementById('smart-scan-phase').classList.remove('hidden');
  });
  await page.waitForTimeout(300);
  
  // Test 11: Progress ring SVG exists
  console.log('\n📋 Test 11: Progress ring SVG');
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
  
  // Test 12: Check all CSS custom properties
  console.log('\n📋 Test 12: CSS custom properties');
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
  
  // Test 13: Check font loading
  console.log('\n📋 Test 13: Font loading');
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