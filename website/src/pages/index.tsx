import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

type Feature = {
  title: string;
  body: ReactNode;
  to: string;
  cta: string;
};

const features: Feature[] = [
  {
    title: 'Permission manifest',
    body: 'Declarative YAML that gates every tool call, file read, and outbound connection. Closed by default.',
    to: '/docs/adrs/004-declarative-yaml-permission-manifest',
    cta: 'ADR-004 →',
  },
  {
    title: 'Hash-chained ledger',
    body: 'Every reasoning step, tool call, and access event is appended to a tamper-evident JSON-LD ledger.',
    to: '/docs/adrs/011-hash-chained-tamper-evident-ledger',
    cta: 'ADR-011 →',
  },
  {
    title: 'Per-turn enforcement',
    body: 'Multi-turn agent loop with triple-bound circuit breaker (turns + tokens + wallclock) and per-turn SPIFFE rebind.',
    to: '/docs/adrs/025-multi-turn-agent-loop-with-triple-bound-circuit-breaker',
    cta: 'ADR-025 →',
  },
  {
    title: 'Compliance matrix',
    body: 'F1–F10 features mapped to OWASP Top 10 for Agentic Applications, NIST AI RMF, CMMC 2.0, and EU AI Act.',
    to: '/docs/COMPLIANCE_MATRIX',
    cta: 'Read the matrix →',
  },
];

function Hero(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <p className={styles.heroSubline}>
          Every enterprise wants AI agents. Every enterprise security team blocks them.
          Aegis-Node is the agent runtime built to survive the security review.
        </p>
        <div className={styles.buttons}>
          <Link
            className="button button--primary button--lg"
            to="/docs/INSTALL">
            Install
          </Link>
          <Link
            className="button button--secondary button--lg"
            to="/docs/adrs/">
            Read the ADRs
          </Link>
          <Link
            className="button button--outline button--secondary button--lg"
            href="https://github.com/tosin2013/aegis-node">
            GitHub
          </Link>
        </div>
      </div>
    </header>
  );
}

function Features(): ReactNode {
  return (
    <section className={styles.features}>
      <div className="container">
        <Heading as="h2" className={styles.sectionTitle}>
          Built around the questions a zero-trust review actually asks
        </Heading>
        <div className="feature-grid">
          {features.map((f) => (
            <Link to={f.to} key={f.title} className={clsx('feature-card', styles.featureCardLink)}>
              <h3>{f.title}</h3>
              <p>{f.body}</p>
              <p className={styles.featureCta}>{f.cta}</p>
            </Link>
          ))}
        </div>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title={`${siteConfig.title} — ${siteConfig.tagline}`}
      description="Open-source AI agent runtime designed to pass a zero-trust infrastructure review.">
      <Hero />
      <main>
        <Features />
      </main>
    </Layout>
  );
}
