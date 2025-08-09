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
  page.on("console", (m) => console.log("[console]", m.text()));

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

  // Toggle help twice
  await page.keyboard.press("KeyH");
  await new Promise((r) => setTimeout(r, 100));
  await page.keyboard.press("KeyH");
  await new Promise((r) => setTimeout(r, 100));

  // Reseed all
  await page.keyboard.press("KeyR");
  await new Promise((r) => setTimeout(r, 100));

  // Pause and resume
  await page.keyboard.press("Space");
  await new Promise((r) => setTimeout(r, 200));
  await page.keyboard.press("Space");

  // Tempo up/down
  await page.keyboard.down("Shift");
  await page.keyboard.press("Equal");
  await page.keyboard.up("Shift");
  await page.keyboard.press("Minus");

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
