DELETE FROM budget_group_assignments WHERE category_id IN ('cat_taxes', 'cat_children', 'cat_pets', 'cat_transfers', 'cat_cash_withdrawal');
DELETE FROM taxonomy_categories WHERE id IN (
  'cat_transfers_own', 'cat_transfers_crypto', 'cat_transfers_broker', 'cat_transfers_savings',
  'cat_taxes_income', 'cat_taxes_property', 'cat_taxes_fines', 'cat_housing_loan', 'cat_bills_insurance',
  'cat_income_transfers_own', 'cat_income_transfers_crypto', 'cat_income_transfers_broker',
  'cat_income_transfers_savings', 'cat_income_prime', 'cat_income_benefits', 'cat_income_pension'
);
DELETE FROM taxonomy_categories WHERE id IN (
  'cat_transfers', 'cat_cash_withdrawal', 'cat_taxes', 'cat_pets', 'cat_children', 'cat_income_transfers'
);
