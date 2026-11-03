import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { REPOSITORY, repoUrl, rewriteLink } from '../.vitepress/repo-links.mts';
import { adrItems, adrTitle, titleOf } from '../.vitepress/sidebar.mts';

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

  it('shortens the title of an ADR', () => {
    expect(adrTitle('ADR 0001: Usage collector architecture')).toBe(
      '0001 Usage collector architecture',
    );
    expect(adrTitle('Something else')).toBe('Something else');
  });

  it('ignores files of docs/adr that are not numbered ADRs', () => {
    const { docs } = repository();
    writeFileSync(join(docs, 'adr/README.md'), '# ADRs\n');
    writeFileSync(join(docs, 'adr/0002-second.md'), 'no heading\n');

    expect(adrItems(docs)).toEqual([
      { text: '0001 Collector', link: '/adr/0001-usage-collector-architecture' },
      { text: '0002-second.md', link: '/adr/0002-second' },
    ]);
  });

  it('fails when docs/adr is missing', () => {
    const empty = mkdtempSync(join(tmpdir(), 'tamias-site-empty-'));
    expect(() => adrItems(empty)).toThrow();
  });

  it('lists every ADR of the repository in number order', () => {
    const items = adrItems(resolve(import.meta.dirname, '../../docs'));

    expect(items.length).toBeGreaterThanOrEqual(2);
    expect(items[0]).toEqual({
      text: '0001 Usage collector architecture',
      link: '/adr/0001-usage-collector-architecture',
    });
    expect(items[1]?.link).toBe('/adr/0002-documentation-site');
    expect(items.map((item) => item.link)).toEqual([...items.map((item) => item.link)].sort());
  });
});
