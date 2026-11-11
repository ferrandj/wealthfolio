DELETE FROM budget_group_assignments WHERE category_id = 'cat_to_categorize';
DELETE FROM taxonomy_categories WHERE id IN ('cat_to_categorize', 'cat_income_to_categorize');
