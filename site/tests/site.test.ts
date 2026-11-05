import { existsSync, mkdirSync, readdirSync, readFileSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { resolveConfig } from 'vitepress';
import { REPOSITORY, repoUrl, rewriteLink } from '../.vitepress/repo-links.mts';
import { isAdr, titleOf } from '../.vitepress/sidebar.mts';

/** A small repository: docs/ with pages, ADRs and a left-out folder, crates with a fixture, a README. */
function repository() {
  const root = mkdtempSync(join(tmpdir(), 'tamias-site-'));
  const docs = join(root, 'docs');
  for (const folder of ['docs/adr', 'docs/drafts', 'crates/core/tests/fixtures/claude', 'spikes']) {
    mkdirSync(join(root, folder), { recursive: true });
  }
  writeFileSync(join(docs, 'sources.md'), '# Usage sources\n');
  writeFileSync(join(docs, 'adr/0001-usage-collector-architecture.md'), '# ADR 0001: Collector\n');
  writeFileSync(join(docs, 'drafts/README.md'), '# Drafts\n');
  writeFileSync(join(root, 'README.md'), '# tamias\n');
  writeFileSync(join(root, 'crates/core/tests/fixtures/claude/usage-ok.json'), '{}\n');
  return { root, docs };
}

describe('rewriteLink', () => {
  const { root, docs } = repository();
  const onSite = (path: string) => !path.startsWith(join(docs, 'drafts'));
  const from = join(docs, 'adr/0001-usage-collector-architecture.md');
  const rewrite = (href: string) => rewriteLink(href, from, root, docs, onSite);

  it('leaves a link to another document of the site alone', () => {
    expect(rewrite('../sources.md')).toBe('../sources.md');
    expect(rewrite('../sources.md#window-behaviour')).toBe('../sources.md#window-behaviour');
    expect(rewrite('./0001-usage-collector-architecture.md')).toBe(
      './0001-usage-collector-architecture.md',
    );
  });

  it('leaves addresses, anchors and absolute paths alone', () => {
    for (const href of [
      'https://github.com/GravionLabs/tamias/issues/1',
      'mailto:a@b.c',
      '#decision',
      '/roadmap',
      'http://localhost:5173',
    ]) {
      expect(rewrite(href)).toBe(href);
    }
  });

  it('sends a link to a file of the repository outside docs to GitHub, with its anchor', () => {
    expect(rewrite('../../README.md#license')).toBe(`${REPOSITORY}/blob/main/README.md#license`);
    expect(rewrite('../../crates/core/tests/fixtures/claude/usage-ok.json')).toBe(
      `${REPOSITORY}/blob/main/crates/core/tests/fixtures/claude/usage-ok.json`,
    );
  });

  it('sends a link to a folder to its tree', () => {
    expect(rewrite('../../spikes')).toBe(`${REPOSITORY}/tree/main/spikes`);
    expect(rewrite('../../crates/core/tests/fixtures/')).toBe(
      `${REPOSITORY}/tree/main/crates/core/tests/fixtures`,
    );
  });

  it('sends a link to a document the site leaves out to GitHub', () => {
    expect(rewrite('../drafts/README.md')).toBe(`${REPOSITORY}/blob/main/docs/drafts/README.md`);
  });

  it('leaves a link to something that does not exist, for the dead link check to report', () => {
    expect(rewrite('../../nowhere/missing.md')).toBe('../../nowhere/missing.md');
    expect(rewrite('./missing.md')).toBe('./missing.md');
  });

  it('leaves an empty path with only a hash alone', () => {
    expect(rewrite('#')).toBe('#');
  });
});

describe('repoUrl', () => {
  it('points at a file or a folder on the main branch', () => {
    expect(repoUrl('a/b.md', false)).toBe(`${REPOSITORY}/blob/main/a/b.md`);
    expect(repoUrl('a', true, '#x')).toBe(`${REPOSITORY}/tree/main/a#x`);
  });

  it('is the tamias repository', () => {
    expect(REPOSITORY).toBe('https://github.com/GravionLabs/tamias');
  });
});

describe('the sidebar', () => {
  it('takes the title of a document from its first heading', () => {
    expect(titleOf('text\n# The title\n## Not this', 'file.md')).toBe('The title');
    expect(titleOf('no heading', 'file.md')).toBe('file.md');
    expect(titleOf('', 'empty.md')).toBe('empty.md');
  });

  it('recognises a path inside the ADR folder', () => {
    const { docs } = repository();
    const adr = join(docs, 'adr');
    expect(isAdr(join(adr, '0001-x.md'), adr)).toBe(true);
    expect(isAdr(adr, adr)).toBe(true);
    expect(isAdr(join(docs, 'adr-notes.md'), adr)).toBe(false);
    expect(isAdr(join(docs, 'sources.md'), adr)).toBe(false);
  });
});

describe('the site without ADRs', () => {
  const repoDocs = resolve(import.meta.dirname, '../../docs');

  it('excludes docs/adr from the build, the sidebar, the nav and the search', async () => {
    const { default: config } = await import('../.vitepress/config.mts');
    const links = JSON.stringify([config.themeConfig?.nav, config.themeConfig?.sidebar]);

    expect(config.srcExclude).toContain('adr/**');
    expect(links).not.toContain('/adr/');
    expect(links).not.toContain('Decisions');
  });

  it('keeps the roadmap and the ADRs off the site: no page, no nav, no sidebar entry, no link', async () => {
    const { default: config } = await import('../.vitepress/config.mts');
    const links = JSON.stringify([config.themeConfig?.nav, config.themeConfig?.sidebar]);
    const site = await resolveConfig(resolve(import.meta.dirname, '..'), 'build', 'production');
    const home = readFileSync(join(repoDocs, 'index.md'), 'utf8');

    expect(config.srcExclude).toContain('roadmap.md');
    expect(site.pages).not.toContain('roadmap.md');
    expect(links).not.toMatch(/roadmap/i);
    expect(home).not.toMatch(/roadmap|\/adr\/|Decisions/i);
  });

  it('shows the icon as logo, favicon and hero image, and the command line page after Installation', async () => {
    const { default: config } = await import('../.vitepress/config.mts');
    const sidebar = JSON.stringify(config.themeConfig?.sidebar);

    expect(config.themeConfig?.logo).toBe('/icon.svg');
    expect(JSON.stringify(config.head)).toContain('icon.svg');
    expect(readFileSync(join(repoDocs, 'index.md'), 'utf8')).toContain('src: /icon.svg');
    expect(existsSync(join(repoDocs, 'public/icon.svg'))).toBe(true);
    expect(sidebar.indexOf('/installation')).toBeLessThan(sidebar.indexOf('/cli'));
    expect(sidebar.indexOf('/cli')).toBeLessThan(sidebar.indexOf('/app'));
  });

  it('keeps no page of the site linking to an ADR page', () => {
    for (const file of readdirSync(repoDocs).filter((name) => name.endsWith('.md'))) {
      const text = readFileSync(join(repoDocs, file), 'utf8');
      expect(text, file).not.toMatch(/\]\(\/adr\/|link:\s*\/adr\//);
    }
  });

  it('produces no ADR page, while the other documents are pages', async () => {
    const site = await resolveConfig(resolve(import.meta.dirname, '..'), 'build', 'production');

    expect(site.pages).toContain('sources.md');
    expect(site.pages).toContain('cli.md');
    expect(site.pages.filter((page) => page.startsWith('adr/'))).toEqual([]);
  });

  it('sends a link from a page to an ADR to GitHub', async () => {
    const { isOnSite } = await import('../.vitepress/config.mts');
    const repoRoot = resolve(import.meta.dirname, '../..');
    const docs = join(repoRoot, 'docs');
    const adr = readdirSync(join(docs, 'adr')).find((name) => name.endsWith('.md'))!;

    expect(isOnSite(join(docs, 'sources.md'))).toBe(true);
    expect(isOnSite(join(docs, 'adr', adr))).toBe(false);
    expect(rewriteLink(`./adr/${adr}`, join(docs, 'index.md'), repoRoot, docs, isOnSite)).toBe(
      `${REPOSITORY}/blob/main/docs/adr/${adr}`,
    );
  });
});
