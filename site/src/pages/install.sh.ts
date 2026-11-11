// The install script, at /install.sh, so that one line fetches and runs it.
// It is the same file a release carries: packaging/install.sh.
import fs from 'node:fs';
import path from 'node:path';
import type { APIRoute } from 'astro';

export const GET: APIRoute = () => {
  // the site is built from its own folder, one below the repository's
  const script = fs.readFileSync(path.resolve(process.cwd(), '../packaging/install.sh'), 'utf8');
  return new Response(script, { headers: { 'content-type': 'text/x-shellscript; charset=utf-8' } });
};
