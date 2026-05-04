-- Synthetic Q2 2026 expense data for the finance audit example.
-- 30 rows across 3 departments; deliberately includes some high-value
-- and unusual entries the agent should flag as anomalies.

CREATE TABLE IF NOT EXISTS expenses (
    id          INTEGER PRIMARY KEY,
    submitted   TEXT NOT NULL,        -- ISO 8601 date
    department  TEXT NOT NULL,
    employee    TEXT NOT NULL,
    vendor      TEXT NOT NULL,
    amount_usd  REAL NOT NULL,
    category    TEXT NOT NULL,
    notes       TEXT
);

DELETE FROM expenses;

INSERT INTO expenses (submitted, department, employee, vendor, amount_usd, category, notes) VALUES
('2026-04-03', 'engineering', 'alice',   'AWS',                  847.20, 'cloud',      'Q2 staging environment'),
('2026-04-04', 'engineering', 'alice',   'GitHub',                21.00, 'tooling',    'pro seat'),
('2026-04-08', 'engineering', 'bob',     'AWS',                  912.55, 'cloud',      'production env Q2'),
('2026-04-12', 'engineering', 'carol',   'Datadog',              280.00, 'observability', NULL),
('2026-04-15', 'engineering', 'bob',     'Anthropic',            127.40, 'ai-services',NULL),
('2026-04-19', 'engineering', 'alice',   'Vercel',                40.00, 'hosting',    NULL),
('2026-04-22', 'engineering', 'dave',    'JetBrains',            299.00, 'tooling',    'IDE renewal'),
('2026-04-28', 'engineering', 'bob',     'Linear',                10.00, 'tooling',    NULL),
('2026-05-02', 'engineering', 'carol',   'AWS',                 1284.30, 'cloud',      'spike — investigate'),
('2026-05-08', 'engineering', 'alice',   'Anthropic',            387.10, 'ai-services',NULL),
('2026-04-04', 'sales',       'eve',     'Salesforce',           750.00, 'crm',        NULL),
('2026-04-09', 'sales',       'frank',   'United Airlines',     1843.00, 'travel',     'NYC client visit'),
('2026-04-11', 'sales',       'eve',     'Hilton',               542.18, 'travel',     'NYC hotel'),
('2026-04-14', 'sales',       'frank',   'Uber',                 187.50, 'travel',     NULL),
('2026-04-17', 'sales',       'grace',   'LinkedIn',             129.99, 'tooling',    'Sales Navigator'),
('2026-04-21', 'sales',       'eve',     'Zoom',                  29.99, 'tooling',    NULL),
('2026-04-25', 'sales',       'frank',   'United Airlines',     2104.20, 'travel',     'SFO client'),
('2026-04-30', 'sales',       'grace',   'Marriott',             612.00, 'travel',     'SFO hotel'),
('2026-05-05', 'sales',       'eve',     'Acme Consulting',     5840.00, 'services',   'unusual vendor — verify'),
('2026-05-09', 'sales',       'frank',   'Uber',                  92.10, 'travel',     NULL),
('2026-04-02', 'marketing',   'henry',   'Google Ads',          3200.00, 'advertising',NULL),
('2026-04-06', 'marketing',   'iris',    'Canva',                119.00, 'tooling',    NULL),
('2026-04-13', 'marketing',   'henry',   'Google Ads',          3450.00, 'advertising',NULL),
('2026-04-16', 'marketing',   'iris',    'Mailchimp',            299.00, 'email',      NULL),
('2026-04-20', 'marketing',   'henry',   'Facebook Ads',        2876.50, 'advertising',NULL),
('2026-04-23', 'marketing',   'iris',    'Adobe',                599.00, 'tooling',    'CC subscription'),
('2026-04-27', 'marketing',   'henry',   'Acme Consulting',    12500.00, 'services',   'unusual vendor — verify'),
('2026-05-01', 'marketing',   'iris',    'Squarespace',          156.00, 'hosting',    NULL),
('2026-05-06', 'marketing',   'henry',   'Google Ads',          2987.30, 'advertising',NULL),
('2026-05-10', 'marketing',   'iris',    'Canva',                119.00, 'tooling',    NULL);
