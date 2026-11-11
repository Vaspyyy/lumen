import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';
import { satteri } from '@astrojs/markdown-satteri';
import theme from './src/lib/shiki-theme.mjs';

// Where the site is served from. On GitHub Pages that is a folder of the
// account's site; with a domain of its own, set SITE_URL and SITE_BASE=/.
const site = process.env.SITE_URL || 'https://vaspyyy.github.io';
const base = process.env.SITE_BASE || '/lumen/';
const prefix = base.replace(/\/$/, '');

// Links and pictures in the pages are written from the site's root
// (/docs/keys/, /shots/thumbs.png): this puts the folder the site lives in
// in front of them.
const baseLinks = () => ({
  name: 'base-links',
  element: {
    filter: ['a', 'img'],
    visit(node, ctx) {
      const name = node.tagName === 'img' ? 'src' : 'href';
      const to = node.properties?.[name];
      if (typeof to === 'string' && to.startsWith('/') && !to.startsWith('//')) {
        ctx.setProperty(node, name, prefix + to);
      }
    },
  },
});

// `Ctrl+Shift+T` in backticks is a key, and is drawn as one.
const NAMES =
  'Ctrl|Shift|Alt|Super|Esc|Enter|Tab|Space|Backspace|Delete|Insert|Home|End|PageUp|PageDown|Arrow|Up|Down|Left|Right|Plus|Minus|F\\d{1,2}|[A-Z0-9]';
const KEY = new RegExp(`^(${NAMES})(\\+(${NAMES}|Click|Drag|Wheel))*$`);
const keys = () => ({
  name: 'keys',
  element: {
    filter: ['code'],
    visit(node, ctx) {
      const text = ctx.textContent(node);
      if (text.length > 1 && KEY.test(text)) {
        ctx.setProperty(node, 'className', ['key']);
      }
    },
  },
});

export default defineConfig({
  site,
  base,
  trailingSlash: 'always',
  integrations: [sitemap()],
  markdown: {
    processor: satteri({ hastPlugins: [baseLinks, keys] }),
    shikiConfig: { theme },
  },
  devToolbar: { enabled: false },
});
