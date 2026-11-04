import { existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitepress';
import { REPOSITORY, repoLinks } from './repo-links.mts';
import { PAGES, isAdr } from './sidebar.mts';

const here = dirname(fileURLToPath(import.meta.url));
const require = createRequire(import.meta.url);
// VitePress brings its own Vue; the documents are outside this package, so Vite is told where to find it.
const vitepressRequire = createRequire(require.resolve('vitepress/package.json'));
const repoRoot = resolve(here, '../..');
const docsRoot = resolve(repoRoot, 'docs');

// Every existing file of docs/ except the ADRs is on the site; a link to anything else in the repository goes to GitHub.
const adrRoot = resolve(docsRoot, 'adr');
const isOnSite = (absolutePath: string) => existsSync(absolutePath) && !isAdr(absolutePath, adrRoot);

// On GitHub Pages the site is at https://<owner>.github.io/<repository>/, so every address needs that prefix.
const base = process.env['SITE_BASE'] ?? '/';

export default defineConfig({
  title: 'tamias',
  description:
    'How much of your Claude and GitHub Copilot usage limits you used today, against the daily budget.',
  lang: 'en',
  base,
  // docs/ is the only source: nothing is copied.
  srcDir: '../docs',
  // The ADRs stay in the repository (ADR 0003): no page, no sidebar entry, no search entry.
  srcExclude: ['adr/**'],
  cleanUrls: true,
  lastUpdated: false,
  // Dead links fail the build (no ignoreDeadLinks), so a broken link in any pull request is found by CI.
  vite: {
    resolve: {
      alias: [
        {
          find: /^vue\/server-renderer$/,
          replacement: vitepressRequire.resolve('vue/server-renderer'),
        },
        { find: /^vue$/, replacement: vitepressRequire.resolve('vue') },
      ],
    },
  },
  markdown: {
    config: (md) => {
      md.use(repoLinks(repoRoot, docsRoot, isOnSite));
    },
  },
  themeConfig: {
    nav: [
      { text: 'Consumers', link: '/consumers' },
      { text: 'Usage sources', link: '/sources' },
      { text: 'Roadmap', link: '/roadmap' },
    ],
    sidebar: [
      { text: 'tamias', items: [...PAGES] },
    ],
    socialLinks: [{ icon: 'github', link: REPOSITORY }],
    search: { provider: 'local' },
    editLink: { pattern: `${REPOSITORY}/edit/main/docs/:path`, text: 'Edit this page on GitHub' },
    outline: [2, 3],
    footer: { message: 'In development. Released under the MIT license.' },
  },
});
