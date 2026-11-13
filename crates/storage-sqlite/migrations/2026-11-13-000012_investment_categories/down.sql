DELETE FROM budget_group_assignments WHERE id = '5d0f2c7e-8a41-4c39-9e0b-2f7a6c1d4b93';
DELETE FROM taxonomy_categories WHERE taxonomy_id = 'spending_categories' AND (id = 'cat_investments' OR parent_id = 'cat_investments');
