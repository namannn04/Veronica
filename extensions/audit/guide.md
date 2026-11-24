# Site Audit

Site Audit crawls a site's sitemap, fetches each page and checks its metadata:
titles, descriptions, canonical links, robots rules, headings, Open Graph and
Twitter cards. Every run is local; nothing is sent anywhere except the pages
being audited.

## Commands

- `vr audit site <site>` crawls up to `--limit` pages (50 by default) and
  reports every issue. `--severity error|warning|notice` hides pages whose
  issues are all milder than that.
- `vr audit page <url>` audits one page without crawling.
- `vr audit sitemap <site>` lists the pages a sitemap names without auditing
  them.

A bare hostname is fine: `https://` is assumed. Every command accepts `--json`.

## Politeness

`--concurrency` defaults to 4 so an audit never reads as a load test against
someone else's server. Raise it only for sites you own.
