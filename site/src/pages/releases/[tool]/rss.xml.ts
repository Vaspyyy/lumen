import rss from '@astrojs/rss';
import type { APIContext } from 'astro';
import { family } from '../../../data/site';
import { products, toolReleases } from '../../../lib/releases';
import { url } from '../../../lib/url';

export async function getStaticPaths() {
  const names = await products();
  return family.filter((tool) => names.includes(tool.name)).map((tool) => ({ params: { tool: tool.name } }));
}

export async function GET(context: APIContext) {
  const tool = context.params.tool!;
  const releases = await toolReleases(tool);
  return rss({
    title: `${tool} releases`,
    description: `What each version of ${tool}, a tool of lumen's family, brought.`,
    site: new URL(url(`/releases/${tool}/`), context.site).href,
    items: releases.map((release) => ({
      title: `${tool} ${release.data.version}: ${release.data.title}`,
      description: release.data.summary,
      pubDate: release.data.date,
      link: new URL(url(`/releases/${tool}/${release.data.version}/`), context.site).href,
    })),
  });
}
