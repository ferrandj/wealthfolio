-- Savings and investments paid from a current account (PEA, life insurance, brokerage account, PER, Livret A, LEP,
-- PEL, CEL, crypto): a spending category of their own, in the Other budget group (like the transfers between own accounts; the Savings group keeps only savings categories). Money sent to an account of
-- one's own stays neutral (the transfer detection marks it), the category says where it went.
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_investments', 'spending_categories', NULL, 'Savings & Investments', 'investments', '#6B8E54', 'PiggyBank', 20),
  ('cat_investments_pea', 'spending_categories', 'cat_investments', 'PEA', 'investments_pea', '#6B8E54', 'TrendingUp', 1),
  ('cat_investments_life', 'spending_categories', 'cat_investments', 'Life Insurance', 'investments_life', '#6B8E54', 'Shield', 2),
  ('cat_investments_broker', 'spending_categories', 'cat_investments', 'Brokerage Account', 'investments_broker', '#6B8E54', 'TrendingUp', 3),
  ('cat_investments_retirement', 'spending_categories', 'cat_investments', 'Retirement Plan', 'investments_retirement', '#6B8E54', 'Briefcase', 4),
  ('cat_investments_livret', 'spending_categories', 'cat_investments', 'Regulated Savings Account', 'investments_livret', '#6B8E54', 'PiggyBank', 5),
  ('cat_investments_lep', 'spending_categories', 'cat_investments', 'LEP', 'investments_lep', '#6B8E54', 'PiggyBank', 6),
  ('cat_investments_pel', 'spending_categories', 'cat_investments', 'Home Savings Plan', 'investments_pel', '#6B8E54', 'Home', 7),
  ('cat_investments_cel', 'spending_categories', 'cat_investments', 'Home Savings Account', 'investments_cel', '#6B8E54', 'Home', 8),
  ('cat_investments_crypto', 'spending_categories', 'cat_investments', 'Crypto', 'investments_crypto', '#6B8E54', 'Bitcoin', 9),
  ('cat_investments_other', 'spending_categories', 'cat_investments', 'Other Investments', 'investments_other', '#6B8E54', 'Wallet', 10);

-- Fixed timestamps: databases created at different times must hold identical seeded rows (sync convergence).
INSERT INTO budget_group_assignments (id, group_id, taxonomy_id, category_id, is_system, created_at, updated_at) VALUES
  ('5d0f2c7e-8a41-4c39-9e0b-2f7a6c1d4b93', '6e25d097-0c73-4521-9407-d47e8dfb73e2', 'spending_categories', 'cat_investments', 1, '2026-11-13T00:00:00.000Z', '2026-11-13T00:00:00.000Z');
