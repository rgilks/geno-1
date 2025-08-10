const puppeteer = require("puppeteer");

(async () => {
  const browser = await puppeteer.launch({
    headless: "new",
    args: [
      // CI-friendly flags: disable sandbox and GPU to avoid kernel/AppArmor limits
      "--no-sandbox",
      "--disable-setuid-sandbox",
      "--disable-dev-shm-usage",
      "--disable-gpu",
      // keep WebGPU feature flag harmlessly; test does not require GPU
      "--enable-unsafe-webgpu",
    ],
  });
  const page = await browser.newPage();
  const logs = [];
  page.on("console", (m) => {
    const t = m.text();
    logs.push(t);
    console.log("[console]", t);
  });

  await page.goto("http://localhost:8080", {
    waitUntil: "networkidle2",
    timeout: 30000,
  });

  await page.waitForSelector("#app-canvas", { timeout: 10000 });
  const box = await page.$eval("#app-canvas", (el) => {
    const r = el.getBoundingClientRect();
    return { x: r.left + r.width / 2, y: r.top + r.height / 2 };
  });

  await page.mouse.click(box.x, box.y);
  await new Promise((r) => setTimeout(r, 400));

  // Overlay should be present initially; close it, then bring it back with 'H'
  const overlayInitially = await page.$('#start-overlay');
  if (!overlayInitially) throw new Error('start overlay not found');
  // Click close to hide
  await page.click('#overlay-close');
  await new Promise((r) => setTimeout(r, 200));
  const overlayHidden = await page.evaluate(() => {
    const el = document.getElementById('start-overlay');
    if (!el) return 'missing';
    const style = el.getAttribute('style') || '';
    return /display:\s*none/.test(style) ? 'hidden' : 'visible';
  });
  if (overlayHidden !== 'hidden') throw new Error('start overlay did not hide after close');
  // Press H to show again
  await page.keyboard.press('KeyH');
  await new Promise((r) => setTimeout(r, 200));
  const overlayShown = await page.evaluate(() => {
    const el = document.getElementById('start-overlay');
    if (!el) return 'missing';
    const style = el.getAttribute('style') || '';
    return /display:\s*none/.test(style) ? 'hidden' : 'visible';
  });
  if (overlayShown !== 'visible') throw new Error('start overlay did not show after H');

  // Engine-dependent checks (only if WebGPU init succeeded and handlers are bound)
  const engineStarted =
    logs.some((l) => l.includes("[gesture] starting systems after click")) &&
    !logs.some((l) => l.includes("WebGPU init error"));
  if (engineStarted) {
    // Reseed all
    await page.keyboard.press("KeyR");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => l.includes("[keys] reseeded all voices")))
      throw new Error("missing reseed log");

    // Pause and resume
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Space");
    await new Promise((r) => setTimeout(r, 120));
    const sawPause =
      logs.some((l) => l.includes("[keys] paused=true")) &&
      logs.some((l) => l.includes("[keys] paused=false"));
    if (!sawPause) throw new Error("missing pause/resume logs");

    // Tempo up/down (no UI assertion; rely on logs/state in future)
    await page.keyboard.down("Shift");
    await page.keyboard.press("Equal");
    await page.keyboard.up("Shift");
    await new Promise((r) => setTimeout(r, 120));
    await page.keyboard.press("Minus");
    await new Promise((r) => setTimeout(r, 120));

    // Master mute toggle
    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[keys\] master muted=true/.test(l)))
      throw new Error("missing master mute= true log");
    // Muted state no longer shown in hint; rely on logs only

    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[keys\] master muted=false/.test(l)))
      throw new Error("missing master mute= false log");
    // Muted state no longer shown in hint; rely on logs only

    // Orbit feature removed; no O-key assertions

    // Click center to toggle mute on the hovered voice (expects a hit)
    await page.mouse.move(box.x, box.y);
    await page.mouse.click(box.x, box.y);
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[click\] toggle mute voice \d+/.test(l)))
      throw new Error("missing toggle mute click log");

    // Alt+Click to solo the same voice
    await page.keyboard.down("Alt");
    await page.mouse.click(box.x, box.y);
    await page.keyboard.up("Alt");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[click\] solo voice \d+/.test(l)))
      throw new Error("missing solo click log");
  } else {
    console.log(
      "[note] engine not started in CI (WebGPU unavailable); skipping R/Space/+/− assertions"
    );
  }

  // Basic assertions
  const hasWebGPU = await page.evaluate(() => !!navigator.gpu);
  console.log("WEBGPU", hasWebGPU);

  await browser.close();
  process.exit(0);
})().catch((err) => {
  console.error("TEST_ERROR", err);
  process.exit(1);
});
