import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

export interface SidebarItem {
  text: string;
  link: string;
}

/** The title of a document: its first `# ` heading, else its file name. */
export function titleOf(markdown: string, fallback: string): string {
  const heading = markdown.match(/^#\s+(.+)$/m);
  return heading ? heading[1].trim() : fallback;
}

/** A short title for an ADR: "ADR 0002: Documentation site" becomes "0002 Documentation site". */
export function adrTitle(title: string): string {
  return title.replace(/^ADR\s+(\d+):\s*/i, '$1 ');
}

/** The ADRs of `docs/adr`, in number order. A new ADR needs no change to the site. */
export function adrItems(docsDir: string): SidebarItem[] {
  const folder = join(docsDir, 'adr');
  return readdirSync(folder)
    .filter((file) => /^\d{4}-.+\.md$/.test(file))
    .sort()
    .map((file) => ({
      text: adrTitle(titleOf(readFileSync(join(folder, file), 'utf8'), file)),
      link: `/adr/${file.replace(/\.md$/, '')}`,
    }));
}

/** The other pages, in the order they are best read. Installation and consumers join when their pages exist. */
export const PAGES: readonly SidebarItem[] = [
  { text: 'Overview', link: '/' },
  { text: 'Consumers', link: '/consumers' },
  { text: 'Usage sources', link: '/sources' },
  { text: 'Roadmap', link: '/roadmap' },
];
