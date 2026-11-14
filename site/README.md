# lumen's website

The front page, the docs and the release notes of lumen and its tools, at <https://vaspyyy.github.io/lumen/>.

It is a static site made with [Astro](https://astro.build). Nothing on it is loaded from another server: the fonts are in the build, and there are no trackers.

```sh
npm install
npm run dev       # a local copy that reloads as you edit
npm run build     # the site, in dist/, with its search index
```

## Where things are

```
src/
  content/docs/       the docs, one Markdown file per page
  content/releases/   the release notes, one Markdown file per version
  content/tool-releases/  the tools' release notes, a folder per tool, a file per version
  data/nav.ts         the order of the docs in the sidebar
  data/site.ts        the tools of the family, in the order their notes are listed
  pages/index.astro   the front page
  components/         the navigation, the search, the terminal on the front page
  layouts/            the frame of every page, and the frame of a docs page
  styles/global.css   colors, type, and how text pages look
public/               pictures and icons, copied as they are
```

## Writing

- **A new docs page** is a Markdown file in `src/content/docs` with a `title` and a `description`, and its name in `src/data/nav.ts`.
- **A new release** is a Markdown file in `src/content/releases`, named after the version. It shows up in the list, in the feed, and on the front page. The release workflow takes the text of the GitHub release from it, and refuses a version that has none.
- **A new release of a tool** (glint, crux, recap, seek, pulse, splice, peek, lens) is a Markdown file in `src/content/tool-releases/<tool>/`, named after the version, with the same head as lumen's notes and `tool:` besides. It shows up in that tool's list at `/releases/<tool>/` and in its feed. Write it before the tool's GitHub release, and take the release's text from it: `packaging/release-notes.sh glint 0.11.4 | gh release create v0.11.4 -R Vaspyyy/glint -F -`. A tool that is new to the family goes into `family` in `src/data/site.ts` first.
- **Links inside the site** are written from its root: `[Keys](/docs/keys/)`. The build puts the folder the site is served from in front of them.
- **A key** in backticks, such as `` `Ctrl+Shift+T` ``, is drawn as a key.

## Publishing

Every push to `main` that touches `site/` is built and published by `.github/workflows/site.yml`, with GitHub Pages.

The site finds out where it is served from when it is built. To move it to a domain of its own, set that domain in the repository's Pages settings: nothing in the site has to change. To build it for another place by hand:

```sh
SITE_URL=https://example.org SITE_BASE=/ npm run build
```
