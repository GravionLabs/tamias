// Takes the screenshots of docs/public/screenshots/ from the Angular app in headless Chromium, with a
// mocked Tauri IPC and synthetic demo data (no real usage, no account). Window size as in tauri.conf.json.
//
// Recreate:
//   cd app && pnpm exec ng build --configuration development
//   npm i --prefix /some/dir playwright-core   (Chromium must be installed, e.g. via Playwright)
//   PLAYWRIGHT_CORE=/some/dir/node_modules/playwright-core/index.mjs node scripts/screenshots.mjs [chromium-executable]
//
// Output: docs/public/screenshots/{calendar,day}-{light,dark}.png
import { createServer } from "node:http";
import { readFile, mkdir } from "node:fs/promises";
import { extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const playwright = await import(process.env.PLAYWRIGHT_CORE ?? "playwright-core");
const { chromium } = playwright.default ?? playwright;

const root = resolve(fileURLToPath(import.meta.url), "../..");
const dist = join(root, "dist/tamias-app/browser");
const out = resolve(root, "../docs/public/screenshots");
const NOW = "2026-11-18T14:30:00Z"; // TZ UTC, a fixed demo "now"
const DAY = "2026-11-17";

const type = {
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
};
const server = createServer(async (req, res) => {
  const path = req.url.split("?")[0] === "/" ? "/index.html" : req.url.split("?")[0];
  try {
    const file = await readFile(join(dist, path === "/" ? "index.html" : path));
    res
      .writeHead(200, { "content-type": type[extname(path)] ?? "application/octet-stream" })
      .end(file);
  } catch {
    res.writeHead(404).end();
  }
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}/`;

// Synthetic data: a deterministic usage per day.
const claudeDays = [];
const copilotDays = [];
const colour = (r) => (r > 1 ? "red" : r >= 0.8 ? "yellow" : "green");
for (let d = 1; d <= 18; d++) {
  const date = `2026-11-${String(d).padStart(2, "0")}`;
  const wave = [0.35, 0.62, 0.9, 0.5, 1.12, 0.75, 0.2][(d * 3) % 7];
  const weekend = [0, 6].includes(new Date(date).getUTCDay());
  const cu = +(10 * wave * (weekend ? 0.3 : 1)).toFixed(1);
  claudeDays.push({ date, usage: cu, budget: 10, ratio: cu / 10, colour: colour(cu / 10) });
  const pu = Math.round(((10 * ((d * 5) % 9)) / 8) * (weekend ? 0.2 : 1) + 3);
  copilotDays.push({ date, usage: pu, budget: 10, ratio: pu / 10, colour: colour(pu / 10) });
}
const curve = (total, steps = 28) => {
  const midnight = Date.parse(`${DAY}T00:00:00Z`);
  const points = [{ at: `${DAY}T00:00:00Z`, usage: 0 }];
  for (let i = 1; i <= steps; i++) {
    const hour = 7 + (i * 13) / steps; // 07:00 .. 20:00
    points.push({
      at: new Date(midnight + hour * 3600e3).toISOString().replace(".000", ""),
      usage: +((total * i ** 1.3) / steps ** 1.3).toFixed(2),
    });
  }
  return points;
};
const answers = {
  week_start: "monday",
  service_status: {
    state: "installed",
    message: null,
    exe: "/Applications/tamias.app/Contents/MacOS/tamias",
  },
  autostart_available: true,
  "plugin:autostart|is_enabled": false,
  "plugin:app|version": "0.7.1",
  today: {
    schema_version: 1,
    generated_at: NOW,
    providers: [
      {
        provider: "claude",
        status: "ok",
        windows: [
          {
            window: "seven_day",
            binding: true,
            unit: "percent",
            used: 54,
            limit: 100,
            resets_at: "2026-11-21T12:00:00Z",
            colour: "yellow",
            today_usage: 8.6,
            budget: 10,
            ratio: 0.86,
            month_usage: 61,
          },
          {
            window: "five_hour",
            binding: false,
            unit: "percent",
            used: 18,
            limit: 100,
            resets_at: "2026-11-18T17:30:00Z",
            colour: "green",
          },
        ],
      },
      {
        provider: "copilot",
        status: "ok",
        windows: [
          {
            window: "monthly",
            binding: true,
            unit: "requests",
            used: 489,
            limit: 2380,
            resets_at: "2026-12-01T00:00:00Z",
            colour: "green",
            today_usage: 6,
            budget: 10,
            ratio: 0.6,
            month_usage: 160,
          },
        ],
      },
    ],
  },
  month: {
    year: 2026,
    month: 11,
    generated_at: NOW,
    providers: [
      { provider: "claude", status: "ok", unit: "percent", days: claudeDays },
      { provider: "copilot", status: "ok", unit: "requests", days: copilotDays },
    ],
  },
  day: {
    date: DAY,
    generated_at: NOW,
    providers: [
      {
        provider: "claude",
        status: "ok",
        unit: "percent",
        points: curve(8.6),
        usage: 8.6,
        budget: 10,
        colour: "yellow",
        five_hour: { used: 18, limit: 100, colour: "green" },
      },
      {
        provider: "copilot",
        status: "ok",
        unit: "requests",
        points: curve(6, 14),
        usage: 6,
        budget: 10,
        colour: "green",
      },
    ],
  },
};

const browser = await chromium.launch({ executablePath: process.argv[2] });
await mkdir(out, { recursive: true });
for (const scheme of ["light", "dark"]) {
  const context = await browser.newContext({
    viewport: { width: 820, height: 600 },
    deviceScaleFactor: 2,
    colorScheme: scheme,
    timezoneId: "UTC",
    locale: "en-US",
  });
  await context.addInitScript((data) => {
    let id = 0;
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => ++id,
      unregisterCallback: () => {},
      invoke: async (cmd) => (cmd.startsWith("plugin:event|") ? 0 : data[cmd]),
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, answers);
  const page = await context.newPage();
  await page.clock.install({ time: new Date(NOW) });
  for (const [name, hash] of [
    ["calendar", "#/"],
    ["day", `#/day/${DAY}`],
  ]) {
    await page.goto(base + hash);
    await page.waitForSelector(
      name === "calendar" ? "[data-colour]" : "[data-testid=panel] app-window-list",
    );
    await page.waitForTimeout(500);
    await page.screenshot({ path: join(out, `${name}-${scheme}.png`) });
  }
  await context.close();
}
await browser.close();
server.close();
