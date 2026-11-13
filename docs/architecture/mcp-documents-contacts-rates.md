# MCP: documents, contacts, rates, goals and portfolios

Every feature of the Documents, Contacts and Rates pages is reachable through
MCP, on both runtimes (server `/mcp` and the embedded Tauri MCP). The tools wrap
the same services as the HTTP API and Tauri commands, so the rules of the app
(duplicates, overlaps, categories chosen by hand kept, own-account transfers…)
apply unchanged.

**Architecture impact.** `AgentEnvironment` gains three optional accessors
(`statement_service`, `contacts_service`, `rates_service`; default `None`, both
hosts return the service they already own). No migration, no new persisted
state, no new network call. Tools run synchronously inside the request, except
`analyze_statement_document`, which reads the PDF on a blocking thread as the
HTTP handler does. After an import, a revalidation or a deletion that removes
operations the health cache is cleared, as the HTTP handlers do. Tested: tool
scopes and audit redaction (unit), scope enforcement and an owner/contacts/rates
round trip (server integration), the whole import → review → validate flow on a
live server. Not tested: the Tauri runtime (compiled and linted only).

## Scopes

| Scope                       | Grants                                                                                      |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| `documents:read` / `:write` | Documents, analysis, review, identities, coverage, loans / import, validate, link, delete   |
| `contacts:read` / `:write`  | Contacts, owners of accounts, shares, overview / create, edit, merge, delete, link accounts |
| `rates:read` / `:write`     | Rate tables and their review / edit, delete, confirm                                        |
| `financial-planning:write`  | Create, edit, delete and fund goals (needs `financial-planning:read`)                       |

A write scope requires its read scope. Portfolio tools use `accounts:write`. The
three read scopes are part of the `read-only` preset (new tokens only: stored
tokens keep the scopes they were created with).

## Tools

- **Documents**: `list_statement_documents`, `get_statement_review_queue`,
  `review_statement_document`, `get_statement_document_content` (≤ 4 MB),
  `list_supported_statement_institutions`, `analyze_statement_document` (upload
  as base64, nothing stored), `import_statement_document` (upload, `overrides`,
  `replaceDocumentIds`), `validate_statement_document`,
  `revalidate_statement_document`, `revalidate_all_statement_documents`,
  `invalidate_all_statement_documents`, `delete_statement_document`,
  `link_statement_document`, `list_account_identities`, `get_account_identity`,
  `save_account_identity`, `get_statement_coverage`,
  `get_outdated_statement_documents`, `set_statement_month_normal`, `get_loans`,
  `set_loan_first_due`, `sync_loan_liabilities` (the liability of each loan
  whose papers hold together: contract terms, early repayments, new instalment,
  and the letters attached to the account of the contract), `link_loan_to_asset`
  (attach a loan to the property, vehicle or other asset it finances: an
  existing one, or a new one with its address as street, postal code and city;
  an empty call detaches it).
- **Contacts**: `list_contacts`, `get_contact`, `create_contact`,
  `update_contact`, `delete_contact`, `merge_contacts`,
  `add_contact_identifier`, `remove_contact_identifier`, `link_contact_account`
  / `unlink_contact_account` (the owner of an account, editable at any time),
  `set_contact_account_share` (joint accounts), `get_contact_overview`,
  `get_contacts_spending`, `list_unresolved_counterparties`,
  `list_missing_own_accounts`.
- **Rates**: `list_rates`, `get_rates_review`, `save_rate`, `delete_rate`,
  `confirm_rates`.
- **Planning and portfolios**: `create_goal`, `update_goal`, `delete_goal`,
  `get_goal_funding`, `save_goal_funding`, `get_goal_plan`, `create_portfolio`,
  `update_portfolio`, `delete_portfolio`.

Typical import by an agent: `analyze_statement_document` →
`import_statement_document` → `get_statement_review_queue` →
`review_statement_document` → `validate_statement_document` (with `decisions`) →
`list_account_identities` / `link_contact_account` to set the owner of each new
account.

## Rule: every feature is reachable through MCP

Anything the app can do is exposed as a tool (or as a field of an existing
tool). Recent additions:

- **Categories remembered**: `validate_statement_document` decisions default to
  `remember: true`; the category chosen for a payee is applied to the other
  operations of the same document and to later imports (source `rule`), never
  over a category chosen by hand.
- **"To categorize later"** (`cat_to_categorize` in `spending_categories`,
  `cat_income_to_categorize` in `income_sources`): a normal category, so the
  operation is counted in totals, charts and analyses; assign it with the usual
  categorization tools.
- **Addresses in parts**: `create_contact` / `update_contact` accept `street`,
  `postalCode`, `city` (or the whole `address`); `save_account_identity` accepts
  `holderStreet`, `holderPostalCode`, `holderCity`. Contacts and identities are
  read back with both the whole address and its parts. The stored text stays
  `"street\npostal code city"`: no migration.
- **Downloading a document**: `get_statement_document_content` returns the
  original file (base64) and its stored name.

## Audit

These tools log only opaque ids and flags (`documentId`, `accountId`,
`removeOperations`…), never names, IBANs, amounts or file contents.

## Not exposed

Retirement plan saving and contribution limit writes (they also drive host-side
work the tool environment does not carry), settings, secrets and AI provider
keys, device sync and Connect, add-ons, backups and restores, and the complete
profile reset (kept behind its five-press button in the app).

`review_document` (HTTP, Tauri and MCP) also returns, for a loan paper, the
whole `loan` (terms, rate, TAEG, plan rows, checks, documents) so the validation
screen and an agent see everything the papers say before validating.

`get_loans` (and the HTTP/Tauri equivalents) now also tell, for each loan, what
is still missing (`gaps`: each names the papers where the fact can be found and,
when the person can simply answer, the answer it accepts), where the date of
instalment n°1 comes from (`firstDueSource`), the answers given (`answers`) and
each instalment already due followed in the bank accounts (`payments`: found /
missing / uncovered, the debited account). Two tools complete them:
`set_loan_answers` (account debited, no early repayment, instalments unchanged)
and `link_loan_document` (attach a letter without loan number to a loan).
Validating one paper of a loan validates the loan, with all its papers.

Loan accounts: each loan paper is attached to its loan account (a liability) as
soon as it is imported (`attach_loan_document`: an existing liability, or a new
account made empty and pending, prefilled from the paper). The account stays
empty (metadata `statement_loan_status = pending`) until the loan is validated
as a whole (`validate_loan`, or validating any of its papers): all its papers
are validated in order (contract, dated tables, letters) and the liability is
filled from them. Invalidating empties it again; deleting its last paper removes
an account made at import, never one the person already had.
`link_loan_document` is replaced by `attach_loan_document`.

Statement continuity: `review_document` returns `continuity` for a statement
(the previous and next statements of its account, missing days, overlap, whether
the opening balance takes up the previous closing balance, older statements not
validated). A statement that does not follow the previous one waits for review
(`reviewReason.chain`). Importing a statement puts back to check the validated
statement right after it and those over the same days (`reviewReason.reopened`);
deleting one does the same for the next one; a new paper of a validated loan
puts the whole loan account back to pending. Loan payments: an instalment with
no debit of its amount gets the debit that most looks like it
(`state: proposed`, a first instalment pro rata), which the person (or
`set_loan_answers` with `confirmedPayments`) confirms.

Loan payments on operations: the review of a statement says, on each operation
that pays a loan, which loan it pays (`loanKey`, `loanLabel`, `loanState`:
matched, proposed or to_attach), validated loan account or not.
`attach_loan_payment` attaches a debit to a loan (it pays the instalment due
nearest to it) or detaches it. `get_operations_to_review` lists every operation
of the imported statements to look at again (doubt, to categorize later, loan to
attach or to confirm) and `categorize_operations` gives categories outside the
review of a document; the Activities page has a Review button for the same list.
