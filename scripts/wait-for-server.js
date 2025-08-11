// Simple wait-until-HTTP-200 helper for CI
const http = require("http");

const URL = process.env.WAIT_URL || "http://localhost:8080/";
const TIMEOUT_MS = Number(process.env.WAIT_TIMEOUT_MS || 15000);
const INTERVAL_MS = 300;

function checkOnce() {
  return new Promise((resolve) => {
    const req = http.get(URL, (res) => {
      res.resume();
      resolve(res.statusCode && res.statusCode >= 200 && res.statusCode < 500);
    });
    req.on("error", () => resolve(false));
    req.setTimeout(2000, () => {
      req.destroy();
      resolve(false);
    });
  });
}

(async () => {
  const start = Date.now();
  for (;;) {
    if (await checkOnce()) {
      process.exit(0);
    }
    if (Date.now() - start > TIMEOUT_MS) {
      console.error(
        `Server did not become ready at ${URL} within ${TIMEOUT_MS}ms`
      );
      process.exit(1);
    }
    await new Promise((r) => setTimeout(r, INTERVAL_MS));
  }
})();
