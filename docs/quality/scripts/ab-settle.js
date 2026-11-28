// Usage: start the baseline server on :47012 and the candidate on :47011
// (`ctx browse . --no-open --port N`), then `node ab-settle.js`.
// Interleaved A/B settle-time comparison: same screens, alternating servers.
const { chromium } = require('playwright');
const SERVERS = { base: 'http://127.0.0.1:47012/', after: 'http://127.0.0.1:47011/' };
const SCREENS = ['#/search?q=parse_browse_args', '#/budget', '#/file/crates/ctx-cli/src/commands/browse.rs', '#/tree'];
const REPS = 5;
(async () => {
  const browser = await chromium.launch();
  const t = {};
  for (let r = 0; r < REPS; r++) {
    for (const hash of SCREENS) {
      for (const [k, base] of (r % 2 ? Object.entries(SERVERS).reverse() : Object.entries(SERVERS))) {
        const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
        const page = await ctx.newPage();
        const t0 = Date.now();
        await page.goto(base + hash, { waitUntil: 'networkidle', timeout: 120000 });
        const ms = Date.now() - t0;
        // Time to first meaningful paint of the app shell (LCP proxy).
        const fcp = await page.evaluate(() => (performance.getEntriesByName('first-contentful-paint')[0] || {}).startTime || -1);
        (t[hash + ' ' + k] ||= []).push([ms, Math.round(fcp)]);
        await ctx.close();
      }
    }
  }
  await browser.close();
  const med = (xs) => xs.slice().sort((a, b) => a - b)[Math.floor(xs.length / 2)];
  for (const [key, v] of Object.entries(t)) {
    console.log(key.padEnd(60), 'networkidle median', med(v.map((x) => x[0])), 'ms  [', v.map((x) => x[0]).join(','), ']  FCP median', med(v.map((x) => x[1])));
  }
})();
