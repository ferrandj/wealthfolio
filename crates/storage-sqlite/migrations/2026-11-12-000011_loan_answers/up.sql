-- What the person tells about a loan when its papers cannot (the account debited, no early repayment, instalments
-- never changed): a small JSON document next to the date of the first instalment.
ALTER TABLE loan_settings ADD COLUMN answers TEXT;
