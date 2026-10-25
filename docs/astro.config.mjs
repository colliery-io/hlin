// The Hlin docs (HLIN-T-0107): Astro Starlight with the Colliery theme
// starlight-aurora, in the Diátaxis sections. Moved from MkDocs.
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import starlightAurora from 'starlight-aurora';
import redirects from './redirects.mjs';
// Written by `plissken render` (plissken.toml). Not in git: run it before a build.
import apiSidebar from './src/content/docs/reference/api/_sidebar.mjs';

// The Pages workflow sets DOCS_SITE and DOCS_BASE. On a pull request and
// locally they are empty, and the site is at /.
const site = process.env.DOCS_SITE || undefined;
const base = process.env.DOCS_BASE || '/';

// Astro does not put the base in front of a redirect's target: do it here.
const atBase = (path) => base.replace(/\/$/, '') + path;

const PROJECT = 'Hlin';
const REPO = 'https://github.com/colliery-io/hlin';

export default defineConfig({
	site,
	base,
	// The old MkDocs addresses send the reader to the new pages.
	redirects: Object.fromEntries(Object.entries(redirects).map(([from, to]) => [from, atBase(to)])),
	integrations: [
		starlight({
			title: PROJECT,
			description: 'One place to work across many systems, assembled by the people who use them.',
			social: [{ icon: 'github', label: 'GitHub', href: REPO }],
			plugins: [starlightAurora({ hyper: true })],
			// Written out. Add a new page here.
			sidebar: [
				{ label: 'Home', link: '/' },
				{ label: 'Tutorials', items: [{ slug: 'tutorials/quickstart', label: 'Quick Start' }] },
				{
					label: 'How-to guides',
					items: [
						{ slug: 'how-to/installation', label: 'Installation' },
						{ slug: 'how-to/shell', label: 'Build the shell at /' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ slug: 'reference', label: 'Overview' },
						{ slug: 'reference/surface-json', label: 'Surface JSON' },
						{ slug: 'reference/shell-css', label: 'Shell class names' },
						apiSidebar,
					],
				},
			],
		}),
	],
});
