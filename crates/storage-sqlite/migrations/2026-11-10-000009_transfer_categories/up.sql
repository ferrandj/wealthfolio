-- Categories that were missing: transfers between own accounts (neutral: neither spent nor earned), cash, taxes,
-- loan repayments, bank bonuses and benefits. Their ids are `cat_<key>`: the statement reader suggests them by key.

-- Spending: top level
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_transfers',       'spending_categories', NULL, 'Transfers Between Own Accounts', 'transfers',       '#6E7F99', 'ArrowLeftRight', 15),
  ('cat_cash_withdrawal', 'spending_categories', NULL, 'Cash Withdrawals',               'cash_withdrawal', '#9C998E', 'Banknote',       16),
  ('cat_taxes',           'spending_categories', NULL, 'Taxes',                          'taxes',           '#4F6B92', 'Receipt',        17),
  ('cat_pets',            'spending_categories', NULL, 'Pets',                           'pets',            '#B74583', 'Heart',          18),
  ('cat_children',        'spending_categories', NULL, 'Children',                       'children',        '#24837B', 'User',           19);

-- Spending: subcategories
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_transfers_own',     'spending_categories', 'cat_transfers', 'Account to Account',       'transfers_own',     '#6E7F99', 'ArrowLeftRight', 1),
  ('cat_transfers_crypto',  'spending_categories', 'cat_transfers', 'To a Crypto Account',      'transfers_crypto',  '#6E7F99', 'Bitcoin',        2),
  ('cat_transfers_broker',  'spending_categories', 'cat_transfers', 'To a Brokerage Account',   'transfers_broker',  '#6E7F99', 'TrendingUp',     3),
  ('cat_transfers_savings', 'spending_categories', 'cat_transfers', 'To a Savings Account',     'transfers_savings', '#6E7F99', 'PiggyBank',      4),
  ('cat_taxes_income',      'spending_categories', 'cat_taxes',     'Income Tax',               'taxes_income',      '#4F6B92', 'Receipt',        1),
  ('cat_taxes_property',    'spending_categories', 'cat_taxes',     'Property Tax',             'taxes_property',    '#4F6B92', 'Home',           2),
  ('cat_taxes_fines',       'spending_categories', 'cat_taxes',     'Fines',                    'taxes_fines',       '#4F6B92', 'AlertCircle',    3),
  ('cat_housing_loan',      'spending_categories', 'cat_housing',   'Home Loan Repayment',      'housing_loan',      '#A35742', 'Home',           6),
  ('cat_bills_insurance',   'spending_categories', 'cat_bills',     'Other Insurance',          'bills_insurance',   '#4F6B92', 'Shield',         5);

-- Income: top level and subcategories
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_income_transfers', 'income_sources', NULL, 'Transfers Between Own Accounts', 'income_transfers', '#6E7F99', 'ArrowLeftRight', 5);
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_income_transfers_own',     'income_sources', 'cat_income_transfers', 'Account to Account',        'income_transfers_own',     '#6E7F99', 'ArrowLeftRight', 1),
  ('cat_income_transfers_crypto',  'income_sources', 'cat_income_transfers', 'From a Crypto Account',     'income_transfers_crypto',  '#6E7F99', 'Bitcoin',        2),
  ('cat_income_transfers_broker',  'income_sources', 'cat_income_transfers', 'From a Brokerage Account',  'income_transfers_broker',  '#6E7F99', 'TrendingUp',     3),
  ('cat_income_transfers_savings', 'income_sources', 'cat_income_transfers', 'From a Savings Account',    'income_transfers_savings', '#6E7F99', 'PiggyBank',      4),
  ('cat_income_prime',             'income_sources', 'cat_income_other',     'Bonuses & Sign-up Offers',  'income_prime',             '#9C998E', 'Award',          5),
  ('cat_income_benefits',          'income_sources', 'cat_income_other',     'Benefits & Allowances',     'income_benefits',          '#9C998E', 'Wallet',         6),
  ('cat_income_pension',           'income_sources', 'cat_income_employment','Pension',                   'income_pension',           '#5A7A3E', 'Briefcase',      4);

-- Fixed timestamps: databases created at different times must hold identical seeded rows (sync convergence).
INSERT INTO budget_group_assignments (id, group_id, taxonomy_id, category_id, is_system, created_at, updated_at) VALUES
  ('7ccc571d-ca1f-4cfb-a6d2-f7606a47e0d0', '032ecb02-5912-42e8-9724-2cd566fc08d5', 'spending_categories', 'cat_taxes', 1, '2026-11-10T00:00:00.000Z', '2026-11-10T00:00:00.000Z'),
  ('bfef11b5-6991-4b65-85d4-086e886d441d', '032ecb02-5912-42e8-9724-2cd566fc08d5', 'spending_categories', 'cat_children', 1, '2026-11-10T00:00:00.000Z', '2026-11-10T00:00:00.000Z'),
  ('bf20d095-bac7-4a98-ba3d-f4707e43909e', 'a409e0d6-9152-49c8-a5b4-a147a8ac636e', 'spending_categories', 'cat_pets', 1, '2026-11-10T00:00:00.000Z', '2026-11-10T00:00:00.000Z'),
  ('591182ce-552a-4b03-ad5b-9509319240be', '6e25d097-0c73-4521-9407-d47e8dfb73e2', 'spending_categories', 'cat_transfers', 1, '2026-11-10T00:00:00.000Z', '2026-11-10T00:00:00.000Z'),
  ('16a01358-6fd8-44d2-bbee-b799fb136154', '6e25d097-0c73-4521-9407-d47e8dfb73e2', 'spending_categories', 'cat_cash_withdrawal', 1, '2026-11-10T00:00:00.000Z', '2026-11-10T00:00:00.000Z');
