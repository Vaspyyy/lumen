// The site can live in a folder (GitHub Pages) or at a domain's root:
// every link inside it goes through here.
const base = import.meta.env.BASE_URL.replace(/\/$/, '');

export function url(path: string): string {
  return base + (path.startsWith('/') ? path : `/${path}`);
}
