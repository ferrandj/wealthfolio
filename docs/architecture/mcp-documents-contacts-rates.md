# MCP: documents, contacts, rates, goals and portfolios

Every feature of the Documents, Contacts and Rates pages is reachable through MCP, on both runtimes (server `/mcp`
and the embedded Tauri MCP). The tools wrap the same services as the HTTP API and Tauri commands, so the rules of the
app (duplicates, overlaps, categories chosen by hand kept, own-account transfers…) apply unchanged.

**Architecture impact.** `AgentEnvironment` gains three optional accessors (`statement_service`, `contacts_service`,
`rates_service`; default `None`, both hosts return the service they already own). No migration, no new persisted state,
no new network call. Tools run synchronously inside the request, except `analyze_statement_document`, which reads the
PDF on a blocking thread as the HTTP handler does. After an import, a revalidation or a deletion that removes
operations the health cache is cleared, as the HTTP handlers do. Tested: tool scopes and audit redaction (unit),
scope enforcement and an owner/contacts/rates round trip (server integration), the whole import → review → validate
flow on a live server. Not tested: the Tauri runtime (compiled and linted only).

## Scopes

| Scope                       | Grants                                                                                      |
| --------------------------- | ------------------------------------------------------------------------------------------- |
| `documents:read` / `:write` | Documents, analysis, review, identities, coverage, loans / import, validate, link, delete   |
| `contacts:read` / `:write`  | Contacts, owners of accounts, shares, overview / create, edit, merge, delete, link accounts |
| `rates:read` / `:write`     | Rate tables and their review / edit, delete, confirm                                        |
| `financial-planning:write`  | Create, edit, delete and fund goals (needs `financial-planning:read`)                       |

A write scope requires its read scope. Portfolio tools use `accounts:write`. The three read scopes are part of the
`read-only` preset (new tokens only: stored tokens keep the scopes they were created with).

## Tools

- **Documents**: `list_statement_documents`, `get_statement_review_queue`, `review_statement_document`,
  `get_statement_document_content` (≤ 4 MB), `list_supported_statement_institutions`, `analyze_statement_document`
  (upload as base64, nothing stored), `import_statement_document` (upload, `overrides`, `replaceDocumentIds`),
  `validate_statement_document`, `revalidate_statement_document`, `revalidate_all_statement_documents`,
  `invalidate_all_statement_documents`, `delete_statement_document`, `link_statement_document`,
  `list_account_identities`, `get_account_identity`, `save_account_identity`, `get_statement_coverage`,
  `get_outdated_statement_documents`, `set_statement_month_normal`, `get_loans`, `set_loan_first_due`.
- **Contacts**: `list_contacts`, `get_contact`, `create_contact`, `update_contact`, `delete_contact`, `merge_contacts`,
  `add_contact_identifier`, `remove_contact_identifier`, `link_contact_account` / `unlink_contact_account` (the owner of
  an account, editable at any time), `set_contact_account_share` (joint accounts), `get_contact_overview`,
  `get_contacts_spending`, `list_unresolved_counterparties`, `list_missing_own_accounts`.
- **Rates**: `list_rates`, `get_rates_review`, `save_rate`, `delete_rate`, `confirm_rates`.
- **Planning and portfolios**: `create_goal`, `update_goal`, `delete_goal`, `get_goal_funding`, `save_goal_funding`,
  `get_goal_plan`, `create_portfolio`, `update_portfolio`, `delete_portfolio`.

Typical import by an agent: `analyze_statement_document` → `import_statement_document` →
`get_statement_review_queue` → `review_statement_document` → `validate_statement_document` (with `decisions`) →
`list_account_identities` / `link_contact_account` to set the owner of each new account.

## Rule: every feature is reachable through MCP

Anything the app can do is exposed as a tool (or as a field of an existing tool). Recent additions:

- **Categories remembered**: `validate_statement_document` decisions default to `remember: true`; the category chosen for a
  payee is applied to the other operations of the same document and to later imports (source `rule`), never over a
  category chosen by hand.
- **"To categorize later"** (`cat_to_categorize` in `spending_categories`, `cat_income_to_categorize` in
  `income_sources`): a normal category, so the operation is counted in totals, charts and analyses; assign it with the
  usual categorization tools.
- **Addresses in parts**: `create_contact` / `update_contact` accept `street`, `postalCode`, `city` (or the whole
  `address`); `save_account_identity` accepts `holderStreet`, `holderPostalCode`, `holderCity`. Contacts and identities
  are read back with both the whole address and its parts. The stored text stays `"street\npostal code city"`: no
  migration.
- **Downloading a document**: `get_statement_document_content` returns the original file (base64) and its stored name.

## Audit

These tools log only opaque ids and flags (`documentId`, `accountId`, `removeOperations`…), never names, IBANs,
amounts or file contents.

## Not exposed

Retirement plan saving and contribution limit writes (they also drive host-side work the tool environment does not
carry), settings, secrets and AI provider keys, device sync and Connect, add-ons, backups and restores, and the
complete profile reset (kept behind its five-press button in the app).
