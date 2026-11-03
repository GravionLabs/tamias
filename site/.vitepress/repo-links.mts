import { existsSync, statSync } from 'node:fs';
import { dirname, relative, resolve, sep } from 'node:path';
import type MarkdownIt from 'markdown-it';

/** The repository on GitHub: links to files the site does not contain point there. */
export const REPOSITORY = 'https://github.com/GravionLabs/tamias';

/** Where a path of the repository (relative to its top folder, with `/`) is on GitHub: a file or a folder. */
export function repoUrl(repoPath: string, isDirectory: boolean, hash = ''): string {
  return `${REPOSITORY}/${isDirectory ? 'tree' : 'blob'}/main/${repoPath}${hash}`;
}

/**
 * What a relative link in a document becomes. A link to another document of the site stays as it is (VitePress turns it
 * into a page and fails the build when the page is missing); a link to anything else in the repository (the README, a
 * file of `crates/`, a folder) becomes a GitHub address, so the Markdown works unchanged on GitHub and on the site. A
 * link to something that is not in the repository is left alone and the dead link check of VitePress reports it.
 */
export function rewriteLink(
  href: string,
  documentPath: string,
  repoRoot: string,
  docsRoot: string,
  isOnSite: (absolutePath: string) => boolean,
): string {
  if (/^([a-z][a-z0-9+.-]*:|#|\/)/i.test(href)) return href;
  const [pathPart, ...rest] = href.split('#');
  const hash = rest.length ? `#${rest.join('#')}` : '';
  if (pathPart === '') return href;
  const target = resolve(dirname(documentPath), decodeURIComponent(pathPart));
  const insideDocs = target === docsRoot || target.startsWith(docsRoot + sep);
  if (insideDocs && (pathPart.endsWith('.md') || !existsSync(target)) && isOnSite(target))
    return href;
  if (!existsSync(target)) return href;
  if (insideDocs && isOnSite(target)) return href;
  const repoPath = relative(repoRoot, target).split(sep).join('/');
  return repoUrl(repoPath, statSync(target).isDirectory(), hash);
}

/** A markdown-it plugin that rewrites the links of every document with `rewriteLink`. */
export function repoLinks(
  repoRoot: string,
  docsRoot: string,
  isOnSite: (absolutePath: string) => boolean,
) {
  return (md: MarkdownIt) => {
    const original = md.renderer.rules.link_open;
    md.renderer.rules.link_open = (tokens, index, options, env, self) => {
      const token = tokens[index];
      const href = token.attrGet('href');
      const documentPath = (env as { path?: string }).path;
      if (href && documentPath) {
        token.attrSet('href', rewriteLink(href, documentPath, repoRoot, docsRoot, isOnSite));
      }
      return original
        ? original(tokens, index, options, env, self)
        : self.renderToken(tokens, index, options);
    };
  };
}
