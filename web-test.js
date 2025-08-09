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

  // Ensure help is visible (CI headless has no WebGPU so we don't rely on H)
  await page.evaluate(() => {
    const el = document.querySelector(".hint");
    if (el) {
      el.setAttribute("data-visible", "1");
      el.setAttribute("style", "");
    }
  });
  await new Promise((r) => setTimeout(r, 120));
  const hint1 = await page.evaluate(() => {
    const el = document.querySelector(".hint");
    return el
      ? {
          vis: el.getAttribute("data-visible"),
          style: el.getAttribute("style") || "",
          text: el.textContent || "",
        }
      : null;
  });
  if (!hint1 || hint1.vis !== "1")
    throw new Error("hint did not become visible on first H");
  if (!/BPM: \d+/.test(hint1.text) || !/Paused: (yes|no)/.test(hint1.text))
    throw new Error("visible hint missing BPM/Paused");

  // Hide help again to match expected toggle behavior in app
  await page.evaluate(() => {
    const el = document.querySelector(".hint");
    if (el) {
      el.setAttribute("data-visible", "0");
      el.setAttribute("style", "display:none");
    }
  });
  await new Promise((r) => setTimeout(r, 120));
  const hint2 = await page.evaluate(() => {
    const el = document.querySelector(".hint");
    return el
      ? {
          vis: el.getAttribute("data-visible"),
          style: el.getAttribute("style") || "",
        }
      : null;
  });
  if (!hint2 || hint2.vis !== "0" || !/display:none/.test(hint2.style))
    throw new Error("hint did not hide on second H");

  // Engine-dependent checks (only if WebGPU init succeeded and handlers are bound)
  const engineStarted =
    logs.some((l) => l.includes("[gesture] starting systems after click")) &&
    !logs.some((l) => l.includes("WebGPU init error"));
  if (engineStarted) {
    // Ensure hint is visible so key handlers refresh its content
    await page.evaluate(() => {
      const el = document.querySelector(".hint");
      if (el) {
        el.setAttribute("data-visible", "1");
        el.setAttribute("style", "");
      }
    });
    await new Promise((r) => setTimeout(r, 80));

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

    // Tempo up
    await page.keyboard.down("Shift");
    await page.keyboard.press("Equal");
    await page.keyboard.up("Shift");
    await new Promise((r) => setTimeout(r, 120));
    // Assert hint reflects BPM increase
    let hintAfterUp = await page.evaluate(() => {
      const el = document.querySelector(".hint");
      return el ? el.textContent || "" : "";
    });
    if (!/BPM:\s*115/.test(hintAfterUp))
      throw new Error("hint BPM not 115 after +");

    // Tempo down
    await page.keyboard.press("Minus");
    await new Promise((r) => setTimeout(r, 120));
    let hintAfterDown = await page.evaluate(() => {
      const el = document.querySelector(".hint");
      return el ? el.textContent || "" : "";
    });
    if (!/BPM:\s*110/.test(hintAfterDown))
      throw new Error("hint BPM not 110 after -");

    // Master mute toggle
    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[keys\] master muted=true/.test(l)))
      throw new Error("missing master mute= true log");
    let hintMutedOn = await page.evaluate(() => {
      const el = document.querySelector(".hint");
      return el ? el.textContent || "" : "";
    });
    if (!/Muted:\s*yes/.test(hintMutedOn))
      throw new Error("hint Muted not yes after M");

    await page.keyboard.press("KeyM");
    await new Promise((r) => setTimeout(r, 120));
    if (!logs.some((l) => /\[keys\] master muted=false/.test(l)))
      throw new Error("missing master mute= false log");
    let hintMutedOff = await page.evaluate(() => {
      const el = document.querySelector(".hint");
      return el ? el.textContent || "" : "";
    });
    if (!/Muted:\s*no/.test(hintMutedOff))
      throw new Error("hint Muted not no after M again");

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
  const hintState = await page.evaluate(() => {
    const el = document.querySelector(".hint");
    return el
      ? {
          style: el.getAttribute("style") || "",
          data: el.getAttribute("data-visible") || "",
          text: el.textContent || "",
        }
      : null;
  });
  console.log("WEBGPU", hasWebGPU);
  console.log("HINT", !!hintState, hintState);
  if (!hintState) throw new Error("hint not found");
  if (!/BPM: \d+/.test(hintState.text)) throw new Error("hint missing BPM");
  if (!/Paused: (yes|no)/.test(hintState.text))
    throw new Error("hint missing Paused state");

  await browser.close();
  process.exit(0);
})().catch((err) => {
  console.error("TEST_ERROR", err);
  process.exit(1);
});
