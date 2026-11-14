export const site = {
  name: 'lumen',
  tagline: 'A terminal that understands what it shows',
  description:
    'lumen is a GPU-rendered terminal for Linux with tabs and splits built in. File names are files you can click and drag, pictures sit between the lines, and a new tab is ready before you look at it.',
  repo: 'https://github.com/Vaspyyy/lumen',
  license: 'GPL-3.0-or-later',
};

/** lumen's family of tools, in the order their release notes are listed. */
export const family = [
  { name: 'glint', what: 'A modern ls: icons, git state, previews, thumbnails.' },
  { name: 'crux', what: 'Runs a build or a test and prints only the verdict and the failures.' },
  { name: 'recap', what: 'A git diff by function and type instead of by hunk.' },
  { name: 'seek', what: 'A code search that knows which function each hit is in.' },
  { name: 'pulse', what: 'Says what the machine is busy with, and whether it is too busy for a measurement.' },
  { name: 'splice', what: 'Checked edits in many files at once: exact text to find and replace, all or nothing.' },
  { name: 'peek', what: 'A short card about a data file or a log: its shape, its errors, its repeated lines.' },
  { name: 'lens', what: 'Facts about pictures and sounds that looking at them doesn’t give.' },
].map((tool) => ({ ...tool, href: `https://github.com/Vaspyyy/${tool.name}` }));

/** The tools on the front page. */
export const tools = ['glint', 'crux', 'seek', 'recap'].map((name) => family.find((tool) => tool.name === name)!);
