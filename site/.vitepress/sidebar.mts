import { sep } from 'node:path';

export interface SidebarItem {
  text: string;
  link: string;
}

/** The title of a document: its first `# ` heading, else its file name. */
export function titleOf(markdown: string, fallback: string): string {
  const heading = markdown.match(/^#\s+(.+)$/m);
  return heading ? heading[1].trim() : fallback;
}

/** Whether a path is inside the ADR folder, which is not part of the site (ADR 0003). */
export function isAdr(absolutePath: string, adrRoot: string): boolean {
  return absolutePath === adrRoot || absolutePath.startsWith(adrRoot + sep);
}

/** The other pages, in the order they are best read. Installation and consumers join when their pages exist. */
export const PAGES: readonly SidebarItem[] = [
  { text: 'Overview', link: '/' },
  { text: 'Installation', link: '/installation' },
  { text: 'Command line', link: '/cli' },
  { text: 'Desktop app', link: '/app' },
  { text: 'Consumers', link: '/consumers' },
  { text: 'Claude Code status line', link: '/statusline' },
  { text: 'Usage sources', link: '/sources' },
];
