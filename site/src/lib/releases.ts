import fs from 'node:fs';
import path from 'node:path';
import { getCollection } from 'astro:content';
import { family } from '../data/site';

/** 0.10.0 comes after 0.9.0. */
function parts(version: string): number[] {
  return version.split('.').map((n) => Number.parseInt(n, 10) || 0);
}

/** Newest version first. */
function newestFirst(a: { data: { version: string } }, b: { data: { version: string } }): number {
  const [x, y] = [parts(a.data.version), parts(b.data.version)];
  for (let i = 0; i < Math.max(x.length, y.length); i++) {
    if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
  }
  return 0;
}

/** lumen's release notes, newest version first. */
export async function allReleases() {
  const releases = await getCollection('releases');
  return releases.sort(newestFirst);
}

/** The release notes of one of the tools, newest version first. */
export async function toolReleases(tool: string) {
  const releases = await getCollection('toolReleases', (release) => release.data.tool === tool);
  for (const release of releases) {
    // a note in the wrong folder or under the wrong name would show as another version
    const where = `${release.data.tool}/${release.data.version}.md`;
    if (release.filePath && !release.filePath.endsWith(where)) {
      throw new Error(`${release.filePath} says it is ${where}`);
    }
  }
  return releases.sort(newestFirst);
}

/** Whose notes there are: lumen, then each tool with at least one note. */
export async function products() {
  const notes = await getCollection('toolReleases');
  const tools = family.filter((tool) => notes.some((note) => note.data.tool === tool.name));
  return ['lumen', ...tools.map((tool) => tool.name)];
}

/** Where the notes of lumen or a tool are, or one version's note. */
export function releasesPath(product: string, version?: string): string {
  const root = product === 'lumen' ? '/releases/' : `/releases/${product}/`;
  return version ? `${root}${version}/` : root;
}

/** Whether `lumen update` keeps the tool current: it is in packaging/tools.txt. */
export function keptCurrent(tool: string): boolean {
  // the site is built from its own folder, one below the repository's
  const list = fs.readFileSync(path.resolve(process.cwd(), '../packaging/tools.txt'), 'utf8');
  return list.split('\n').some((line) => line.trim() === tool);
}

export function longDate(date: Date): string {
  return date.toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });
}
