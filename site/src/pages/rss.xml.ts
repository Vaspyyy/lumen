import rss from '@astrojs/rss';
import type { APIContext } from 'astro';
import { allReleases } from '../lib/releases';
import { url } from '../lib/url';

export async function GET(context: APIContext) {
  const releases = await allReleases();
  return rss({
    title: 'lumen releases',
    description: 'What each version of lumen, the terminal, brought.',
    site: new URL(url('/'), context.site).href,
    items: releases.map((release) => ({
      title: `lumen ${release.data.version}: ${release.data.title}`,
      description: release.data.summary,
      pubDate: release.data.date,
      link: new URL(url(`/releases/${release.data.version}/`), context.site).href,
    })),
  });
}
