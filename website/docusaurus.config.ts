import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// Aegis-Node public documentation site.
// Source of truth for docs content is the sibling `../docs/` tree
// (ADRs, INSTALL.md, COMPATIBILITY_CHARTER.md, ...). Edits land in the
// repo as normal Markdown PRs — Vercel rebuilds the site on merge.

const config: Config = {
  title: 'Aegis-Node',
  tagline: 'The AI agent runtime designed to pass a zero-trust infrastructure review.',
  favicon: 'img/favicon.svg',

  // Public site origin. Override per-environment via VERCEL_URL env if needed.
  url: 'https://aegis-node.dev',
  baseUrl: '/',

  organizationName: 'tosin2013',
  projectName: 'aegis-node',

  // Existing markdown is GitHub-relative. Warn instead of throw so the
  // initial deploy succeeds; broken-link cleanup is a follow-up task.
  onBrokenLinks: 'warn',
  onBrokenMarkdownLinks: 'warn',
  onBrokenAnchors: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  markdown: {
    // Treat .md as plain CommonMark, not MDX. Existing ADRs/INSTALL/etc.
    // were authored for GitHub-flavored Markdown; MDX parsing on JSX-like
    // tokens (e.g. generic types in code spans) would break them.
    format: 'md',
    mermaid: true,
  },

  themes: ['@docusaurus/theme-mermaid'],

  presets: [
    [
      'classic',
      {
        docs: {
          // Point at the repo-root docs/ tree, not website/docs.
          path: '../docs',
          routeBasePath: 'docs',
          sidebarPath: './sidebars.ts',
          editUrl:
            'https://github.com/tosin2013/aegis-node/tree/main/docs/',
          showLastUpdateAuthor: false,
          showLastUpdateTime: true,
        },
        blog: {
          path: 'blog',
          routeBasePath: 'blog',
          showReadingTime: true,
          blogTitle: 'Aegis-Node blog',
          blogDescription: 'Release notes, security write-ups, and design rationale.',
          postsPerPage: 10,
          feedOptions: {
            type: ['rss', 'atom'],
            title: 'Aegis-Node blog',
            description: 'Release notes, security write-ups, and design rationale.',
            copyright: `Copyright © ${new Date().getFullYear()} Aegis-Node contributors. Apache-2.0 licensed.`,
          },
          editUrl:
            'https://github.com/tosin2013/aegis-node/tree/main/website/blog/',
        },
        theme: {
          customCss: './src/css/custom.css',
        },
        sitemap: {
          changefreq: 'weekly',
          priority: 0.5,
          filename: 'sitemap.xml',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/social-card.png',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Aegis-Node',
      logo: {
        alt: 'Aegis-Node',
        src: 'img/logo.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'mainSidebar',
          position: 'left',
          label: 'Docs',
        },
        {to: '/blog', label: 'Blog', position: 'left'},
        {
          href: 'https://github.com/tosin2013/aegis-node/blob/main/RELEASE_PLAN.md',
          label: 'Roadmap',
          position: 'left',
        },
        {
          href: 'https://github.com/tosin2013/aegis-node',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Docs',
          items: [
            {label: 'Install', to: '/docs/INSTALL'},
            {label: 'Chat surface', to: '/docs/CHAT'},
            {label: 'Compatibility charter', to: '/docs/COMPATIBILITY_CHARTER'},
            {label: 'Compliance matrix', to: '/docs/COMPLIANCE_MATRIX'},
            {label: 'ADRs', to: '/docs/adrs/'},
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub Discussions',
              href: 'https://github.com/tosin2013/aegis-node/discussions',
            },
            {
              label: 'Issues',
              href: 'https://github.com/tosin2013/aegis-node/issues',
            },
            {
              label: 'Contributing',
              href: 'https://github.com/tosin2013/aegis-node/blob/main/CONTRIBUTING.md',
            },
          ],
        },
        {
          title: 'More',
          items: [
            {label: 'Blog', to: '/blog'},
            {
              label: 'Release plan',
              href: 'https://github.com/tosin2013/aegis-node/blob/main/RELEASE_PLAN.md',
            },
            {
              label: 'License (Apache-2.0)',
              href: 'https://github.com/tosin2013/aegis-node/blob/main/LICENSE',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} Aegis-Node contributors. Apache-2.0 licensed.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['bash', 'yaml', 'toml', 'rust', 'go', 'json', 'protobuf', 'diff'],
    },
    docs: {
      sidebar: {
        hideable: true,
        autoCollapseCategories: true,
      },
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
