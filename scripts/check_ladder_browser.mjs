// Optional validation tool; Playwright and Chrome are not package dependencies.
// node scripts/check_ladder_browser.mjs CASES.json CHROME PLAYWRIGHT_CORE_MODULE
import assert from "node:assert/strict";
import { readFile, writeFile } from "node:fs/promises";
import { resolve, dirname, basename } from "node:path";
import { pathToFileURL } from "node:url";
import { createRequire } from "node:module";
const [casesArg, chrome, modulePath] = process.argv.slice(2);
if (!casesArg || !chrome || !modulePath) throw Error("Expected CASES.json CHROME PLAYWRIGHT_CORE_MODULE");
const { chromium } = createRequire(import.meta.url)(resolve(modulePath));
const casesPath = resolve(casesArg), output = dirname(casesPath);
const cases = JSON.parse(await readFile(casesPath, "utf8"));
const browser = await chromium.launch({ executablePath: resolve(chrome), headless: true });
const results = [];
try {
  const page = await browser.newPage({ viewport: { width: 1440, height: 1050 } });
  const errors = [], requests = [];
  page.on("pageerror", error => errors.push(error.message));
  page.on("request", request => { if (/^https?:/.test(request.url())) requests.push(request.url()); });
  for (const entry of cases) {
    await page.goto(pathToFileURL(resolve(output, entry.file)).href);
    const doc = entry.document, program = entry.program;
    assert.equal(await page.locator("#name").textContent(), program.logical_name);
    assert.equal(await page.evaluate(() => window.attacked), undefined);
    assert.equal(await page.locator("#canvas svg").count(), 1);
    assert.equal(await page.locator("#instructions button").count(), program.instructions.length);
    assert.equal(await page.locator("#status").textContent(), doc.complete ? "対応範囲の接続を構築済み" : "部分表示 · 未解決箇所あり");
    let clicked = 0;
    for (const element of doc.elements) {
      const node = page.locator(`#${element.id}`);
      await node.scrollIntoViewIfNeeded();
      await node.click(); // Actual pointer hit, not a script calling the selection handler.
      const ins = program.instructions[element.instruction], source = ins.source;
      assert.equal(await page.locator("#selection").getAttribute("data-instruction"), String(element.instruction));
      assert.equal(await page.locator("#selection").getAttribute("data-offset"), String(source.offset));
      assert.equal(await page.locator("#selection").getAttribute("data-length"), String(source.length));
      const detail = await page.locator("#selection").textContent();
      assert.ok(detail.includes(program.source_sha256));
      assert.ok(detail.includes(`source: ${program.origin} / ${source.source_id ?? "file"}`));
      const expectedMatches = doc.elements.filter(e => e.instruction === element.instruction).length;
      assert.equal(await page.locator("#canvas .selected").count(), expectedMatches);
      clicked++;
    }
    const button = page.locator("#instructions button").last();
    await button.focus();
    await button.press("Enter");
    assert.equal(await page.locator("#selection").getAttribute("data-instruction"), String(program.instructions.length - 1));
    await page.locator("#zoom").fill("75");
    assert.equal(await page.locator("#canvas svg").evaluate(el => el.getBoundingClientRect().width), doc.width * .75);
    assert.equal(await page.locator("#zoom-value").textContent(), "75%");
    if (!doc.complete) assert.ok((await page.locator("#findings").textContent()).length > 0);
    await page.locator("#zoom").fill("100");
    await page.evaluate(() => { document.getElementById("canvas").scrollTo(0,0); document.querySelector("aside").scrollTo(0,0); });
    if (doc.elements.length) await page.locator(`#${doc.elements[0].id}`).click();
    const screenshot = basename(entry.file, ".html") + ".png";
    await page.screenshot({ path: resolve(output, screenshot) });
    results.push({ file: entry.file, complete: doc.complete, pointer_selections: clicked, keyboard: "passed", zoom: "passed", escaped_text: "passed", screenshot });
  }
  assert.deepEqual(errors, []);
  assert.deepEqual(requests, []);
  const report = { browser: await browser.version(), browser_errors: errors, network_requests: requests, cases: results };
  await writeFile(resolve(output,"browser-results.json"), JSON.stringify(report,null,2)+"\n");
  console.log(JSON.stringify(report));
} finally { await browser.close(); }
