// Optional Playwright/Chrome gate. Actual pointer hits and source bytes are checked.
import assert from "node:assert/strict";
import {readFile, writeFile} from "node:fs/promises";
import {resolve, dirname} from "node:path";
import {pathToFileURL} from "node:url";
import {createRequire} from "node:module";
const [manifest, chrome, modulePath] = process.argv.slice(2);
const {chromium} = createRequire(import.meta.url)(resolve(modulePath));
const cases = JSON.parse(await readFile(manifest, "utf8"));
const browser = await chromium.launch({executablePath:resolve(chrome),headless:true});
const results=[];
try {
  const page = await browser.newPage({viewport:{width:1440,height:1000}});
  const errors=[],requests=[];
  page.on("pageerror",e=>errors.push(e.message));
  page.on("request",r=>{if(/^https?:/.test(r.url()))requests.push(r.url());});
  for (const c of cases) {
    await page.goto(pathToFileURL(resolve(dirname(manifest),c.file)).href);
    assert.equal(await page.evaluate(()=>window.attacked),undefined);
    assert.equal(await page.locator("svg .item").count(),c.elements.length);
    for (const expected of c.elements) {
      const element=page.locator(`#${expected.id}`);
      await element.scrollIntoViewIfNeeded();
      await element.click();
      const selected=JSON.parse(await page.locator("#selection").textContent());
      assert.equal(selected.sha256,c.sha256);
      assert.equal(selected.element.id,expected.id);
      assert.deepEqual(selected.element.source,expected.source);
      assert.equal(selected.bytes_preview,expected.bytes_preview);
      assert.equal(await page.locator(".selected").count(),1);
    }
    const first=page.locator(`#${c.elements[0].id}`);
    await first.focus();await first.press("Enter");
    assert.equal(JSON.parse(await page.locator("#selection").textContent()).element.id,c.elements[0].id);
    await page.screenshot({path:resolve(dirname(manifest),c.file+".png")});
    results.push({file:c.file,pointer_selections:c.elements.length,keyboard:"passed"});
  }
  assert.deepEqual(errors,[]);assert.deepEqual(requests,[]);
  const report={browser:await browser.version(),results,errors,requests};
  await writeFile(resolve(dirname(manifest),"structured-browser.json"),JSON.stringify(report,null,2)+"\n");
  console.log(JSON.stringify(report));
}finally{await browser.close();}
