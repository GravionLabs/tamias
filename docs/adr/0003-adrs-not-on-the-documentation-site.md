# ADR 0003: ADRs are not published on the documentation site

- Status: Proposed
- Date: 2026-11-04
- Issues: #73
- Supersedes: [ADR 0002](0002-documentation-site.md) (in part: the ADRs as pages of the site)

## Context

[ADR 0002](0002-documentation-site.md) publishes `docs/` as a VitePress site and lists the ADRs among its contents, with a sidebar read from `docs/adr`. The ADRs are decision records for the maintainers, not documentation for the users of tamias, and they change the site's search results and navigation without helping a reader.

## Decision

- The site build excludes `docs/adr/**` (`srcExclude`): no page, no sidebar entry, no search entry.
- `docs/adr/` stays in the repository. A link from a page of the site to an ADR points to the file on GitHub (`repo-links`), because the ADR is not a page of the site.
- The rest of ADR 0002 stands: VitePress in `site/`, `docs/` as the only source, the dead link check, publishing by `pages.yml`.

## Consequences

- A new ADR needs no change to the site, and does not appear on it.
- Links to ADRs on the site leave the site for GitHub.

## Alternatives

- **Keep the ADRs on the site:** status quo. Rejected: noise for users.
- **Delete the links only:** the ADRs would still be built and searchable. Rejected.
