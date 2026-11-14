import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { z } from 'astro/zod';
import { family } from './data/site';

const docs = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/docs' }),
  schema: z.object({
    title: z.string(),
    /** One sentence: under the title, and what search engines show. */
    description: z.string(),
    /** A shorter name for the sidebar. */
    nav: z.string().optional(),
  }),
});

const releases = defineCollection({
  loader: glob({ pattern: '*.md', base: './src/content/releases' }),
  schema: z.object({
    version: z.string(),
    date: z.coerce.date(),
    title: z.string(),
    summary: z.string(),
  }),
});

// The notes of the tools of lumen's family, one folder per tool:
// tool-releases/glint/0.11.3.md. The same fields as lumen's, and the tool.
const toolReleases = defineCollection({
  loader: glob({ pattern: '*/*.md', base: './src/content/tool-releases' }),
  schema: z.object({
    tool: z.enum(family.map((tool) => tool.name) as [string, ...string[]]),
    version: z.string(),
    date: z.coerce.date(),
    title: z.string(),
    summary: z.string(),
  }),
});

export const collections = { docs, releases, toolReleases };
