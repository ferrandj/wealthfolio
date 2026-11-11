-- A category for money spent or received that the person will sort out later. It is a category like any other: the
-- operation is already counted in totals, charts and analyses, and nothing waits for it to be replaced.
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_to_categorize',        'spending_categories', NULL, 'To Categorize Later', 'to_categorize',        '#C9A227', 'Clock', 98),
  ('cat_income_to_categorize', 'income_sources',      NULL, 'To Categorize Later', 'income_to_categorize', '#C9A227', 'Clock', 98);

-- Fixed id and timestamps: databases created at different times must hold identical seeded rows (sync convergence).
INSERT INTO budget_group_assignments (id, group_id, taxonomy_id, category_id, is_system, created_at, updated_at) VALUES
  ('df0d7465-211c-4c9a-b427-1f4348acdfc3', '6e25d097-0c73-4521-9407-d47e8dfb73e2', 'spending_categories', 'cat_to_categorize', 1, '2026-11-11T00:00:00.000Z', '2026-11-11T00:00:00.000Z');
