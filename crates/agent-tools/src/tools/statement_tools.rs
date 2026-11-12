//! MCP tools for bank statements and other account documents: everything the Documents page does — upload and
//! analyze a file, import it, review and validate its operations, revalidate, link or delete a document, edit an
//! account's identity (holders, IBAN, BIC…), see the months still missing, and follow the loans found in documents.
//!
//! A file travels as base64 in `contentBase64`, as it does on the HTTP API. The tools wrap the statements service, so
//! the rules (duplicates, overlaps, categories chosen by hand are kept…) are the ones of the app.

use base64::Engine;
use serde::Deserialize;
use serde_json::json;
use wealthfolio_core::statements::{
    supported_documents, AccountIdentityInput, ReviewDecision, StatementImportRequest,
    MAX_DOCUMENT_BYTES,
};

use super::required_nullable;
use super::service_tools::{fail, need, no_args, object, reply, service_tool};

/// The largest original a tool returns inline: a base64 string this big already fills a model's context.
const MAX_INLINE_CONTENT_BYTES: i64 = 4 * 1024 * 1024;

fn decode(content_base64: &str) -> Result<Vec<u8>, crate::tool::AgentToolError> {
    base64::engine::general_purpose::STANDARD
        .decode(content_base64.trim())
        .map_err(|_| {
            crate::tool::AgentToolError::InvalidInput(
                "contentBase64 is not valid base64".to_string(),
            )
        })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentArgs {
    document_id: String,
}

fn document_schema() -> serde_json::Value {
    object(
        json!({"documentId": {"type": "string", "description": "A statement document id (list_statement_documents)."}}),
        &["documentId"],
    )
}

fn file_properties() -> serde_json::Value {
    json!({
        "fileName": {"type": "string", "description": "The file's name, with its extension (the PDF, CSV or other statement)."},
        "contentBase64": {"type": "string", "description": "The file's bytes, base64 encoded (standard alphabet). 25 MB at most."},
        "adapterId": {"type": ["string", "null"], "description": "Force an analysis adapter (see list_supported_statement_institutions); leave out to detect it."}
    })
}

service_tool!(
    ListStatementDocuments, "list_statement_documents", Read, additive = false,
    scopes = [DocumentsRead],
    description = "List the stored documents (statements and other account documents) with their status (pending review, validated…), period, account, institution and how many operations still need a look.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.statement_service(), "Statements")?.list_documents().map_err(fail)?)
    }
);

service_tool!(
    GetStatementReviewQueue, "get_statement_review_queue", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The documents waiting for a review: their operations have categories that are guesses to confirm. Use review_statement_document on one, then validate_statement_document.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.statement_service(), "Statements")?.review_queue().map_err(fail)?)
    }
);

service_tool!(
    ReviewStatementDocument, "review_statement_document", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The review of one document: each statement and operation it holds, the activity it became, the category it has now, the category guessed from its label, the counterparty and the flags (unknown account, old catch-all category…).",
    schema = document_schema(),
    |env, args| {
        let args: DocumentArgs = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        reply(&service.review_document(&args.document_id).await.map_err(fail)?)
    }
);

service_tool!(
    GetStatementDocumentContent, "get_statement_document_content", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The original bytes of a stored document, base64 encoded, with its metadata. Refused above 4 MB: such a file does not fit a model's context.",
    schema = document_schema(),
    |env, args| {
        let args: DocumentArgs = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        let (document, content) = service.get_document_content(&args.document_id).map_err(fail)?;
        if document.size_bytes > MAX_INLINE_CONTENT_BYTES {
            return Err(crate::tool::AgentToolError::InvalidInput(format!(
                "The document is {} bytes: too large to return inline (limit {MAX_INLINE_CONTENT_BYTES})",
                document.size_bytes
            )));
        }
        reply(&json!({
            "document": document,
            "contentBase64": base64::engine::general_purpose::STANDARD.encode(content),
        }))
    }
);

service_tool!(
    ListSupportedStatementInstitutions, "list_supported_statement_institutions", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The institutions and document types the analysis adapters read, with the adapter ids usable as adapterId.",
    schema = no_args(),
    |_env, _args| {
        reply(&supported_documents())
    }
);

service_tool!(
    AnalyzeStatementDocument, "analyze_statement_document", Read, additive = false,
    scopes = [DocumentsRead],
    description = "Read a file without storing anything: the adapter found, each statement in it (institution, holders, IBAN, period, balances), its operations, the account each would go to or create, and the stored document when this exact file was already imported. Always do this before import_statement_document.",
    schema = object(file_properties(), &["fileName", "contentBase64"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            file_name: String,
            content_base64: String,
            adapter_id: Option<String>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        let content = decode(&args.content_base64)?;
        if content.len() > MAX_DOCUMENT_BYTES {
            return Err(crate::tool::AgentToolError::InvalidInput(format!(
                "The file is larger than {MAX_DOCUMENT_BYTES} bytes"
            )));
        }
        // reading a PDF is CPU work: keep it off the async workers
        let analysis = tokio::task::spawn_blocking(move || {
            service.analyze(&args.file_name, &content, args.adapter_id.as_deref())
        })
        .await
        .map_err(fail)?
        .map_err(fail)?;
        reply(&analysis)
    }
);

service_tool!(
    ImportStatementDocument, "import_statement_document", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Import a file as the Documents page does: store the original, create or find each account, add the operations (duplicates are skipped), guess their categories. `overrides` decide per statement (index from analyze_statement_document): put it in another accountId, name the account to create (newAccountName), skip it, or set the identity. `replaceDocumentIds` removes the operations of stored statements this one replaces after an overlap; allowDuplicateDocument imports again a file whose bytes are already stored. Afterwards use get_statement_review_queue / review_statement_document / validate_statement_document, and set who owns a new account with link_contact_account.",
    schema = object(json!({
        "fileName": file_properties()["fileName"],
        "contentBase64": file_properties()["contentBase64"],
        "adapterId": file_properties()["adapterId"],
        "overrides": {"type": "array", "description": "One entry per statement to adjust.", "items": object(json!({
            "index": {"type": "integer", "minimum": 0},
            "accountId": {"type": ["string", "null"], "description": "Put the operations in this account."},
            "newAccountName": {"type": ["string", "null"], "description": "Name of the account to create when none matches."},
            "skip": {"type": "boolean"},
            "identity": {"type": ["object", "null"], "description": "Identity of the account: institution, holderName, holderAddress (or holderStreet, holderPostalCode, holderCity), iban, bic, bankCode, branchCode, accountNumber, ribKey."}
        }), &["index"])},
        "allowDuplicateDocument": {"type": "boolean"},
        "replaceDocumentIds": {"type": "array", "items": {"type": "string"}}
    }), &["fileName", "contentBase64"]),
    |env, args| {
        let request: StatementImportRequest = serde_json::from_value(args)?;
        decode(&request.content_base64)?;
        let service = need(env.statement_service(), "Statements")?;
        let result = service.import(request).await.map_err(fail)?;
        env.health_service().clear_cache().await;
        reply(&result)
    }
);

service_tool!(
    ValidateStatementDocument, "validate_statement_document", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Validate a reviewed document. `decisions` gives the category chosen for each activity to change (activityId, taxonomyId, categoryId, remember (default true) = give the same category to the other operations with the same payee unless one was chosen by hand); activities without a decision keep their guessed category, which the validation confirms.",
    schema = object(json!({
        "documentId": {"type": "string"},
        "decisions": {"type": "array", "items": object(json!({
            "activityId": {"type": "string"},
            "taxonomyId": {"type": "string", "description": "spending_categories or income_sources."},
            "categoryId": {"type": "string"},
            "remember": {"type": "boolean", "description": "Default true"}
        }), &["activityId", "taxonomyId", "categoryId"])}
    }), &["documentId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            document_id: String,
            #[serde(default)]
            decisions: Vec<ReviewDecision>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        reply(&service.validate_document(&args.document_id, args.decisions).await.map_err(fail)?)
    }
);

service_tool!(
    RevalidateStatementDocument, "revalidate_statement_document", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Run the validation of one document again with today's rules (categories, own-account transfers, unknown accounts): what was chosen by hand is kept.",
    schema = document_schema(),
    |env, args| {
        let args: DocumentArgs = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        let document = service.revalidate_document(&args.document_id).await.map_err(fail)?;
        env.health_service().clear_cache().await;
        reply(&document)
    }
);

service_tool!(
    RevalidateAllStatementDocuments, "revalidate_all_statement_documents", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Run the validation of every stored document again with today's rules. Returns how many were validated, are pending or failed.",
    schema = no_args(),
    |env, _args| {
        let result = need(env.statement_service(), "Statements")?.revalidate_all().await.map_err(fail)?;
        env.health_service().clear_cache().await;
        reply(&result)
    }
);

service_tool!(
    InvalidateAllStatementDocuments, "invalidate_all_statement_documents", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Put every validated document back in the review queue (the app's 'Tout invalider'). Returns how many were reset. Nothing is deleted.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.statement_service(), "Statements")?.invalidate_all().await.map_err(fail)?)
    }
);

service_tool!(
    DeleteStatementDocument, "delete_statement_document", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Delete a stored document. With removeOperations true, the operations it created are deleted too; otherwise they stay.",
    schema = object(json!({
        "documentId": {"type": "string"},
        "removeOperations": {"type": "boolean", "description": "Also delete the operations imported from it (default false)."}
    }), &["documentId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            document_id: String,
            #[serde(default)]
            remove_operations: bool,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        service.delete_document(&args.document_id, args.remove_operations).await.map_err(fail)?;
        if args.remove_operations {
            env.health_service().clear_cache().await;
        }
        reply(&json!({"deleted": args.document_id, "operationsRemoved": args.remove_operations}))
    }
);

service_tool!(
    LinkStatementDocument,
    "link_statement_document",
    Write,
    additive = false,
    scopes = [DocumentsWrite],
    description =
        "Attach a stored document to a Wealthfolio account, or detach it with accountId null.",
    schema = object(
        json!({
            "documentId": {"type": "string"},
            "accountId": {"type": ["string", "null"], "description": "The account, or null to detach."}
        }),
        &["documentId", "accountId"]
    ),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            document_id: String,
            #[serde(deserialize_with = "required_nullable")]
            account_id: Option<String>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        reply(
            &service
                .link_document(&args.document_id, args.account_id)
                .await
                .map_err(fail)?,
        )
    }
);

service_tool!(
    ListAccountIdentities, "list_account_identities", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The identity stored with each account that has one: institution, holder name, the list of holders (a joint account has several), address, IBAN, BIC, bank and branch codes, account number and RIB key.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.statement_service(), "Statements")?.list_identities().map_err(fail)?)
    }
);

service_tool!(
    GetAccountIdentity,
    "get_account_identity",
    Read,
    additive = false,
    scopes = [DocumentsRead],
    description = "The identity stored with one account (null when it has none).",
    schema = object(json!({"accountId": {"type": "string"}}), &["accountId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            account_id: String,
        }
        let args: Args = serde_json::from_value(args)?;
        reply(
            &need(env.statement_service(), "Statements")?
                .get_identity(&args.account_id)
                .map_err(fail)?,
        )
    }
);

service_tool!(
    SaveAccountIdentity, "save_account_identity", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Edit an account's identity. A field left out stays as it is; an empty string clears it. For a joint account, join the holders in holderName with ' ET ' (e.g. 'M DUPONT JEAN ET MME DUPONT MARIE').",
    schema = object(json!({
        "accountId": {"type": "string"},
        "institution": {"type": "string"},
        "holderName": {"type": "string"},
        "holderAddress": {"type": "string", "description": "The whole address (\"street\\npostal code city\"), or use the three parts below."},
        "holderStreet": {"type": "string", "description": "Number and street. A part given replaces the same part; the others stay."},
        "holderPostalCode": {"type": "string"},
        "holderCity": {"type": "string"},
        "iban": {"type": "string"},
        "bic": {"type": "string"},
        "bankCode": {"type": "string"},
        "branchCode": {"type": "string"},
        "accountNumber": {"type": "string"},
        "ribKey": {"type": "string"}
    }), &["accountId"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            account_id: String,
            #[serde(flatten)]
            input: AccountIdentityInput,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        reply(&service.save_identity(&args.account_id, args.input).await.map_err(fail)?)
    }
);

service_tool!(
    GetStatementCoverage, "get_statement_coverage", Read, additive = false,
    scopes = [DocumentsRead],
    description = "For each account, which months have a statement, which are missing, and where a balance does not follow from the previous statement.",
    schema = no_args(),
    |env, _args| {
        reply(&need(env.statement_service(), "Statements")?.coverage().map_err(fail)?)
    }
);

service_tool!(
    GetOutdatedStatementDocuments,
    "get_outdated_statement_documents",
    Read,
    additive = false,
    scopes = [DocumentsRead],
    description =
        "Documents imported by an older version of an adapter: re-importing them would read more.",
    schema = no_args(),
    |env, _args| {
        reply(
            &need(env.statement_service(), "Statements")?
                .outdated_documents()
                .map_err(fail)?,
        )
    }
);

service_tool!(
    SetStatementMonthNormal, "set_statement_month_normal", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Mark a month without statement as normal (the account was empty or closed) or as missing again. Returns the coverage.",
    schema = object(json!({
        "accountId": {"type": "string"},
        "month": {"type": "string", "description": "YYYY-MM"},
        "normal": {"type": "boolean"}
    }), &["accountId", "month", "normal"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args { account_id: String, month: String, normal: bool }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        service.set_month_normal(&args.account_id, &args.month, args.normal).await.map_err(fail)?;
        reply(&service.coverage().map_err(fail)?)
    }
);

service_tool!(
    GetLoans, "get_loans", Read, additive = false,
    scopes = [DocumentsRead],
    description = "The loans found in loan documents (amortization tables, offers): lender, capital, rate, instalments, what remains due and the payments matched in the statements.",
    schema = no_args(),
    |env, _args| {
        let today = chrono::Utc::now().date_naive();
        reply(&need(env.statement_service(), "Statements")?.loans(today).map_err(fail)?)
    }
);

service_tool!(
    SyncLoanLiabilities, "sync_loan_liabilities", Write, additive = false,
    scopes = [DocumentsWrite, HoldingsWrite],
    description = "Create or bring up to date the liability (Passifs) of every loan whose papers hold together (contract terms, early repayments, new instalment) and attach the loan's letters and tables to the account of its contract. Done automatically after each import; run it to repeat it. Returns the ids of the liabilities created or changed.",
    schema = object(json!({}), &[]),
    |env, _args| {
        let service = need(env.statement_service(), "Statements")?;
        reply(&service.sync_loan_liabilities().await.map_err(fail)?)
    }
);

service_tool!(
    LinkLoanToAsset, "link_loan_to_asset", Write, additive = false,
    scopes = [DocumentsWrite, HoldingsWrite],
    description = "Attach a loan (loanKey from get_loans; its liability exists once its papers hold together) to the asset it finances: an existing asset (assetId, from the holdings) or a new one (newAsset: kind PROPERTY, VEHICLE or OTHER, name, value, and the address as street, postalCode and city, or as one text in `address`). Send neither assetId nor newAsset to detach the loan. Returns the liability id, the linked asset id and whether the asset was created.",
    schema = object(json!({
        "loanKey": {"type": "string"},
        "assetId": {"type": ["string", "null"], "description": "An existing property, vehicle or other asset."},
        "newAsset": {
            "type": ["object", "null"],
            "properties": {
                "kind": {"type": "string", "enum": ["PROPERTY", "VEHICLE", "OTHER"]},
                "name": {"type": "string"},
                "address": {"type": ["string", "null"], "description": "The whole address (street, then postal code and city on a second line)."},
                "street": {"type": ["string", "null"]},
                "postalCode": {"type": ["string", "null"]},
                "city": {"type": ["string", "null"]},
                "value": {"type": "number", "description": "What it is worth today."},
                "valueDate": {"type": ["string", "null"], "description": "YYYY-MM-DD, today by default."},
                "purchasePrice": {"type": ["number", "null"]},
                "purchaseDate": {"type": ["string", "null"]}
            },
            "required": ["kind", "name", "value"]
        }
    }), &["loanKey"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            loan_key: String,
            #[serde(default)]
            asset_id: Option<String>,
            #[serde(default)]
            new_asset: Option<wealthfolio_core::statements::NewLoanAsset>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        let link = wealthfolio_core::statements::LoanAssetLink {
            asset_id: args.asset_id,
            new_asset: args.new_asset,
        };
        reply(&service.link_loan_asset(&args.loan_key, link).await.map_err(fail)?)
    }
);

service_tool!(
    LinkLoanDocument, "link_loan_document", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Attach a bank letter that carries no loan number (an early repayment notice, kind loan_event) to a loan (documentId from list_documents, loanKey from get_loans), or detach it with a null loanKey so that it finds its loan by its figures again. Returns the loans.",
    schema = object(json!({
        "documentId": {"type": "string"},
        "loanKey": {"type": ["string", "null"], "description": "The loan's key, or null to detach."}
    }), &["documentId", "loanKey"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            document_id: String,
            #[serde(deserialize_with = "required_nullable")]
            loan_key: Option<String>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        service.link_loan_document(&args.document_id, args.loan_key).await.map_err(fail)?;
        reply(&service.loans(chrono::Utc::now().date_naive()).map_err(fail)?)
    }
);

service_tool!(
    SetLoanAnswers, "set_loan_answers", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Tell what a loan's papers cannot say (loanKey from get_loans; each gap of the loan names the answer it accepts): the account the instalments are debited from (debitAccountId, an account id from get_accounts), noEarlyRepayment (true when no early repayment was ever made), paymentsUnchanged (true when the instalments never changed since the start). Replaces the previous answers. Returns the loans.",
    schema = object(json!({
        "loanKey": {"type": "string"},
        "debitAccountId": {"type": ["string", "null"], "description": "The account the instalments are debited from, or null."},
        "noEarlyRepayment": {"type": "boolean"},
        "paymentsUnchanged": {"type": "boolean"}
    }), &["loanKey"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            loan_key: String,
            #[serde(default)]
            debit_account_id: Option<String>,
            #[serde(default)]
            no_early_repayment: bool,
            #[serde(default)]
            payments_unchanged: bool,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        let answers = wealthfolio_core::statements::LoanAnswers {
            debit_account_id: args.debit_account_id,
            no_early_repayment: args.no_early_repayment,
            payments_unchanged: args.payments_unchanged,
        };
        service.set_loan_answers(&args.loan_key, answers).await.map_err(fail)?;
        reply(&service.loans(chrono::Utc::now().date_naive()).map_err(fail)?)
    }
);

service_tool!(
    SetLoanFirstDue, "set_loan_first_due", Write, additive = false,
    scopes = [DocumentsWrite],
    description = "Set the date of a loan's first instalment when the document does not say it (loanKey from get_loans), or clear it with null. Returns the loans.",
    schema = object(json!({
        "loanKey": {"type": "string"},
        "firstDue": {"type": ["string", "null"], "description": "YYYY-MM-DD, or null to clear."}
    }), &["loanKey", "firstDue"]),
    |env, args| {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Args {
            loan_key: String,
            #[serde(deserialize_with = "required_nullable")]
            first_due: Option<chrono::NaiveDate>,
        }
        let args: Args = serde_json::from_value(args)?;
        let service = need(env.statement_service(), "Statements")?;
        service.set_loan_first_due(&args.loan_key, args.first_due).await.map_err(fail)?;
        reply(&service.loans(chrono::Utc::now().date_naive()).map_err(fail)?)
    }
);
