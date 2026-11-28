// Fixed-condition UI audit: same screens, viewports, themes, data (this repo @ HEAD).
// Usage: node ui-audit.js <label> [port]   (OUT_DIR, AXE_JS env vars)
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const label = process.argv[2] || 'before';
const port = process.argv[3] || '47011';
const BASE = `http://127.0.0.1:${port}/`;
const OUT = path.join(process.env.OUT_DIR || process.cwd(), 'audit-' + label);
fs.mkdirSync(OUT, { recursive: true });
// axe-core 4.10.2: `npm pack axe-core@4.10.2 && tar xzf axe-core-4.10.2.tgz`, then AXE_JS=package/axe.min.js
const AXE = fs.readFileSync(process.env.AXE_JS || path.join(__dirname, 'axe/package/axe.min.js'), 'utf8');

const SCREENS = [
  ['tree', '#/tree'],
  ['file-rs', '#/file/crates/ctx-cli/src/commands/browse.rs'],
  ['file-md', '#/file/README.md'],
  ['search', '#/search?q=parse_browse_args'],
  ['dir', '#/dir/crates'],
  ['budget', '#/budget'],
  ['gitlog', '#/gitlog'],
  ['largest', '#/largest'],
  ['pins', '#/pins'],
];
const VIEWPORTS = [
  ['desktop', { width: 1440, height: 900 }],
  ['mobile', { width: 390, height: 844 }],
];
const THEMES = ['dark', 'light'];

(async () => {
  const browser = await chromium.launch();
  const results = [];
  for (const th of THEMES) {
    for (const [vpName, vp] of VIEWPORTS) {
      const ctx = await browser.newContext({ viewport: vp, deviceScaleFactor: 1, colorScheme: th, reducedMotion: 'reduce' });
      await ctx.addInitScript((t) => { try { localStorage.setItem('ctx-theme', t); } catch {} }, th);
      for (const [name, hash] of SCREENS) {
        const page = await ctx.newPage();
        const errors = [];
        page.on('pageerror', (e) => errors.push(String(e)));
        page.on('console', (m) => { if (m.type() === 'error') errors.push(m.text()); });
        const t0 = Date.now();
        await page.goto(BASE + hash, { waitUntil: 'networkidle' });
        await page.waitForTimeout(800);
        const settleMs = Date.now() - t0;
        await page.evaluate(AXE);
        const axe = await page.evaluate(async () => {
          const r = await window.axe.run(document, { runOnly: { type: 'tag', values: ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'] }, resultTypes: ['violations'] });
          return r.violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.length, sample: v.nodes.slice(0, 3).map((n) => n.target.join(' ')) }));
        });
        const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
        const theme = await page.evaluate(() => document.documentElement.dataset.theme);
        await page.screenshot({ path: path.join(OUT, `${name}-${vpName}-${th}.png`) });
        results.push({ screen: name, viewport: vpName, theme: th, appliedTheme: theme, settleMs, overflowPx: overflow, errors, axe });
        await page.close();
      }
      await ctx.close();
    }
  }
  await browser.close();
  fs.writeFileSync(path.join(OUT, 'results.json'), JSON.stringify(results, null, 2));
  let total = 0; const byRule = {};
  for (const r of results) for (const v of r.axe) { total += v.nodes; byRule[v.id] = (byRule[v.id] || 0) + v.nodes; }
  console.log('runs', results.length, 'axe violation nodes', total, JSON.stringify(byRule));
  console.log('overflow>0:', results.filter((r) => r.overflowPx > 0).map((r) => `${r.screen}/${r.viewport}/${r.theme}:${r.overflowPx}`).join(' ') || 'none');
  console.log('console errors:', results.filter((r) => r.errors.length).map((r) => `${r.screen}/${r.viewport}/${r.theme}:${r.errors.length}`).join(' ') || 'none');
  console.log('themes applied:', [...new Set(results.map((r) => r.appliedTheme))].join(','));
})();
