-- Notary fees and the money paid to a notary for a purchase (deposit, contribution at the signing): a housing
-- category of their own.
INSERT INTO taxonomy_categories (id, taxonomy_id, parent_id, name, key, color, icon, sort_order) VALUES
  ('cat_housing_notary', 'spending_categories', 'cat_housing', 'Notary', 'housing_notary', '#A35742', 'FileText', 7);
