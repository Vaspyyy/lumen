// The docs, in the order they are read. `slug` is the file's name in
// src/content/docs; the first page is the one at /docs/.
export const docsNav = [
  {
    title: 'Start',
    pages: ['index', 'install', 'first-steps'],
  },
  {
    title: 'Using lumen',
    pages: ['tabs-and-splits', 'launching', 'commands', 'machine', 'files-and-links', 'images', 'search', 'keys', 'mouse'],
  },
  {
    title: 'On your phone',
    pages: ['phone', 'kept-terminals', 'phone-security'],
  },
  {
    title: 'Make it yours',
    pages: ['configuration', 'looks', 'fonts', 'colors', 'instant-tabs'],
  },
  {
    title: 'Reference',
    pages: ['command-line', 'for-programs', 'tools', 'troubleshooting'],
  },
  {
    title: 'Project',
    pages: ['how-it-works', 'roadmap', 'contributing'],
  },
];

export const docsOrder = docsNav.flatMap((group) => group.pages);

export function docHref(slug: string): string {
  return slug === 'index' ? '/docs/' : `/docs/${slug}/`;
}
