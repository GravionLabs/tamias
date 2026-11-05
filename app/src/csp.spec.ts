import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string): any =>
  JSON.parse(readFileSync(new URL(path, import.meta.url), "utf8"));

/**
 * `pnpm tauri dev` applies no CSP (`devCsp: null`) and the development build has no critical CSS,
 * so this class of bug only shows in a release build: Tauri adds a hash for the inline `<style>`
 * that Angular's critical CSS inlining writes into index.html to `style-src`, browsers then ignore
 * `'unsafe-inline'`, and the runtime component styles are blocked (bug #138).
 */
describe("release CSP and production build", () => {
  const tauri = read("../src-tauri/tauri.conf.json");
  const angular = read("../angular.json");
  const styleSrc: string = tauri.app.security.csp["style-src"];
  const production = angular.projects["tamias-app"].architect.build.configurations.production;

  it("relies on 'unsafe-inline' for component styles", () => {
    expect(styleSrc).toContain("'unsafe-inline'");
  });

  it("emits no inline <style> into index.html while style-src relies on 'unsafe-inline'", () => {
    const styles = production.optimization?.styles;
    const inlinesCritical =
      styles === undefined || styles === true || styles.inlineCritical !== false;
    if (styleSrc.includes("'unsafe-inline'")) {
      expect(inlinesCritical).toBe(false);
    }
  });
});
