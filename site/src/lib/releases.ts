import { getCollection } from 'astro:content';

/** 0.10.0 comes after 0.9.0. */
function parts(version: string): number[] {
  return version.split('.').map((n) => Number.parseInt(n, 10) || 0);
}

/** The release notes, newest version first. */
export async function allReleases() {
  const releases = await getCollection('releases');
  return releases.sort((a, b) => {
    const [x, y] = [parts(a.data.version), parts(b.data.version)];
    for (let i = 0; i < Math.max(x.length, y.length); i++) {
      if ((x[i] ?? 0) !== (y[i] ?? 0)) return (y[i] ?? 0) - (x[i] ?? 0);
    }
    return 0;
  });
}

export function longDate(date: Date): string {
  return date.toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric', timeZone: 'UTC' });
}
