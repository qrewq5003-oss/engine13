// The check of the province map without Tauri (docs/engine2_map.md §37.7): starts `vite`,
// opens the stand in Chromium and reads the DOM. Not a part of CI: Playwright is taken from
// the machine (`require('playwright')`), not from package.json.
//
//     python tools/map/ui_check/make_fixtures.py
//     node tools/map/ui_check/check.cjs [directory for the screenshots]
const { chromium } = require('playwright');
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const REPO = path.resolve(__dirname, '..', '..', '..');
const SCENARIO = 'milan_1477';
const STAND = 'http://localhost:1420/dev/province-map.html';
const shots = process.argv[2] ? path.resolve(process.argv[2]) : null;

const read = (rel) => JSON.parse(fs.readFileSync(path.join(REPO, rel), 'utf8'));
const style = read(`public/geodata2/${SCENARIO}/map_style.json`);
const start = read(`dev/fixtures/${SCENARIO}_start.json`);
const after = read(`dev/fixtures/${SCENARIO}_after.json`);
const geo = read(`public/geodata2/${SCENARIO}/provinces.geojson`);

const results = [];
function check(name, ok, detail) {
  results.push({ name, ok });
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ` — ${detail}` : ''}`);
}

async function waitForServer(ms) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    try {
      const res = await fetch(STAND);
      if (res.ok) return;
    } catch {
      // not up yet
    }
    await new Promise((r) => setTimeout(r, 300));
  }
  throw new Error(`vite did not answer at ${STAND}`);
}

const fills = (page) =>
  page.$$eval('path.pm-province', (els) =>
    Object.fromEntries(els.map((e) => [e.getAttribute('data-province'), (e.getAttribute('fill') || '').toLowerCase()])),
  );

function expectedFills(view) {
  return Object.fromEntries(view.provinces.map((p) => [p.id, style.actors[p.owner].toLowerCase()]));
}

function differing(a, b) {
  return Object.keys(a).filter((k) => a[k] !== b[k]);
}

async function run(page) {
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  // Every request of the page, to see that the map asks the network for nothing but the
  // stand's own files: no tiles, no fonts, no library from elsewhere.
  const requested = [];
  page.on('request', (r) => requested.push(r.url()));
  await page.goto(STAND);
  await page.waitForSelector('path.pm-province[data-province]');

  // ---- the start
  const atStart = await fills(page);
  const drawn = Object.keys(atStart).length;
  check('81 provinces are drawn, each with its id', drawn === 81 && geo.features.every((f) => f.properties.id in atStart), `${drawn} paths`);
  const wrongStart = differing(expectedFills(start), atStart);
  check('every province has the colour of its owner (start)', wrongStart.length === 0, wrongStart.join(', '));
  check('no province is hatched at the start', (await page.$$('path.pm-nodata')).length === 0);
  const withCapital = start.actors.filter((a) => a.capital !== null).length;
  check('a capital mark for every polity with a capital', (await page.$$('.pm-capital')).length === withCapital, `${withCapital} polities`);
  check('no unit at the start', (await page.$$('.pm-unit')).length === 0);
  const own = await page.$eval('path[data-province="milano"]', (e) => e.getAttribute('stroke'));
  const foreign = await page.$eval('path[data-province="venezia"]', (e) => e.getAttribute('stroke'));
  check("the viewer's land is outlined apart from another's", own !== foreign, `${own} / ${foreign}`);
  await page.hover('path[data-province="milano"]');
  const info = await page.textContent('[data-testid="pm-info"]');
  const milan = start.actors.find((a) => a.id === 'milan').name;
  check('the hint names the province and its owner from the inputs', info.includes(milan), info);
  await page.click('path[data-province="milano"]');
  check('a click selects the province', (await page.textContent('[data-testid="selected"]')).includes('milano'));
  await page.mouse.move(2, 2);
  if (shots) await page.screenshot({ path: path.join(shots, 'province-map-start.png') });

  // ---- after a turn: one province moved, three units
  await page.click('[data-testid="case-after"]');
  await page.waitForFunction((n) => document.querySelectorAll('.pm-unit').length === n, after.units.length);
  const atAfter = await fills(page);
  const moved = differing(atStart, atAfter);
  check('after the move the colour changed for one province only', moved.length === 1 && moved[0] === 'bergamo', moved.join(', '));
  check('the moved province has the colour of its new owner', atAfter.bergamo === style.actors.milan.toLowerCase(), atAfter.bergamo);
  check('three units are drawn', (await page.$$('.pm-unit')).length === 3);
  const boxes = await page.$$eval('.pm-unit', (els) =>
    els.map((e) => {
      const r = e.getBoundingClientRect();
      return `${Math.round(r.x)}:${Math.round(r.y)}`;
    }),
  );
  check('two units in one province do not cover each other', new Set(boxes).size === 3, boxes.join(' '));
  if (shots) await page.screenshot({ path: path.join(shots, 'province-map-after.png') });

  // ---- a polity fell: its land is another's, it stays in the view with alive = false
  await page.click('[data-testid="case-fallen"]');
  await page.waitForFunction((n) => document.querySelectorAll('.pm-capital').length === n, withCapital - 1);
  const atFall = await fills(page);
  check('the fallen polity has no capital mark', (await page.$$('.pm-capital')).length === withCapital - 1);
  check('its land has the colour of the one who took it', atFall.lucca === style.actors.florence.toLowerCase(), atFall.lucca);

  // ---- a province the inputs do not name
  await page.click('[data-testid="case-missing"]');
  await page.waitForSelector('path.pm-nodata');
  const hatched = await page.$$eval('path.pm-nodata', (els) => els.map((e) => e.getAttribute('data-province')));
  check('the province out of the inputs is hatched, and it alone', hatched.length === 1 && hatched[0] === 'firenze', hatched.join(', '));
  const fill = await page.$eval('path[data-province="firenze"]', (e) => getComputedStyle(e).fill);
  check('its fill is the hatch, not the colour of the start', fill.includes('pm-hatch'), fill);
  if (shots) await page.screenshot({ path: path.join(shots, 'province-map-missing.png') });

  // ---- the map's file does not load
  await page.click('[data-testid="case-broken"]');
  await page.waitForSelector('[data-testid="pm-error"]');
  check('a map that did not load is an error on the screen', (await page.textContent('[data-testid="pm-error"]')).includes('Карта не загружена'));
  check('and no map is drawn in its place', (await page.$$('path.pm-province')).length === 0);

  // ---- a map without the provinces' points: an older file, another scenario's
  await page.click('[data-testid="case-old"]');
  await page.waitForSelector('[data-testid="pm-error"]');
  const why = await page.textContent('[data-testid="pm-error"]');
  check('a map without the points is refused with the reason', why.includes('Карта не загружена') && why.includes('label'), why);
  check('and is not drawn in part', (await page.$$('path.pm-province')).length === 0);

  check('no script error on the page', errors.length === 0, errors.join(' | '));
  const outside = requested.filter((u) => !u.startsWith('http://localhost:1420/') && !u.startsWith('data:'));
  check('no request leaves the stand: no tiles, nothing from the network', outside.length === 0, `${requested.length} requests${outside.length ? ': ' + outside.slice(0, 3).join(' ') : ''}`);
}

async function main() {
  const vite = spawn('npx', ['vite', '--port', '1420', '--strictPort'], { cwd: REPO, stdio: 'ignore' });
  let browser;
  try {
    await waitForServer(60000);
    browser = await chromium.launch();
    const page = await browser.newPage({ viewport: { width: 1280, height: 860 }, deviceScaleFactor: shots ? 2 : 1 });
    await run(page);
  } finally {
    if (browser) await browser.close();
    vite.kill('SIGTERM');
  }
  const failed = results.filter((r) => !r.ok).length;
  console.log(`${results.length - failed} of ${results.length} checks passed`);
  process.exitCode = failed === 0 ? 0 : 1;
}

main().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
