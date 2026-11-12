// Bank statements & account documents
import type {
  AccountCoverage,
  LoanAssetLink,
  LoanAssetLinked,
  LoanView,
  OutdatedDocuments,
  AccountIdentity,
  AccountIdentityInput,
  LoanAccountChoice,
  LoanAnswers,
  DocumentReview,
  RevalidationResult,
  ReviewDecision,
  StatementAnalysis,
  StatementDocument,
  StatementDocumentContent,
  StatementImportRequest,
  StatementImportResult,
  SupportedInstitution,
} from "@/lib/types/statements";

import { invoke } from "./platform";

export const analyzeStatement = async (
  fileName: string,
  contentBase64: string,
  adapterId?: string | null,
): Promise<StatementAnalysis> => {
  return invoke<StatementAnalysis>("analyze_statement", {
    fileName,
    contentBase64,
    adapterId: adapterId ?? null,
  });
};

export const importStatement = async (
  request: StatementImportRequest,
): Promise<StatementImportResult> => {
  return invoke<StatementImportResult>("import_statement", { request });
};

export const listStatementDocuments = async (): Promise<StatementDocument[]> => {
  return invoke<StatementDocument[]>("list_statement_documents");
};

export const getStatementDocumentContent = async (
  id: string,
): Promise<StatementDocumentContent> => {
  return invoke<StatementDocumentContent>("get_statement_document_content", { id });
};

export const deleteStatementDocument = async (
  id: string,
  removeOperations = false,
): Promise<void> => {
  return invoke<void>("delete_statement_document", { id, removeOperations });
};

export const linkStatementDocument = async (
  id: string,
  accountId: string | null,
): Promise<StatementDocument> => {
  return invoke<StatementDocument>("link_statement_document", { id, accountId });
};

export const listAccountIdentities = async (): Promise<AccountIdentity[]> => {
  return invoke<AccountIdentity[]>("list_account_identities");
};

export const getAccountIdentity = async (accountId: string): Promise<AccountIdentity | null> => {
  return invoke<AccountIdentity | null>("get_account_identity", { accountId });
};

export const saveAccountIdentity = async (
  accountId: string,
  input: AccountIdentityInput,
): Promise<AccountIdentity> => {
  return invoke<AccountIdentity>("save_account_identity", { accountId, input });
};

export const listSupportedStatements = async (): Promise<SupportedInstitution[]> => {
  return invoke<SupportedInstitution[]>("list_supported_statements");
};

export const getStatementReviewQueue = async (): Promise<StatementDocument[]> => {
  return invoke<StatementDocument[]>("get_statement_review_queue");
};

export const reviewStatementDocument = async (id: string): Promise<DocumentReview> => {
  return invoke<DocumentReview>("review_statement_document", { id });
};

export const validateStatementDocument = async (
  id: string,
  decisions: ReviewDecision[] = [],
): Promise<StatementDocument> => {
  return invoke<StatementDocument>("validate_statement_document", { id, decisions });
};

export const revalidateStatementDocument = async (id: string): Promise<StatementDocument> => {
  return invoke<StatementDocument>("revalidate_statement_document", { id });
};

export const revalidateAllStatementDocuments = async (): Promise<RevalidationResult> => {
  return invoke<RevalidationResult>("revalidate_all_statement_documents");
};

/** Every validated statement goes back to waiting for review; returns how many. */
export const invalidateAllStatementDocuments = async (): Promise<number> => {
  return invoke<number>("invalidate_all_statement_documents");
};

export const getStatementOutdated = async (): Promise<OutdatedDocuments> => {
  return invoke<OutdatedDocuments>("get_statement_outdated");
};

export const getStatementCoverage = async (): Promise<AccountCoverage[]> => {
  return invoke<AccountCoverage[]>("get_statement_coverage");
};

export const setStatementMonthNormal = async (
  accountId: string,
  month: string,
  normal: boolean,
): Promise<AccountCoverage[]> => {
  return invoke<AccountCoverage[]>("set_statement_month_normal", { accountId, month, normal });
};

export const getStatementLoans = async (): Promise<LoanView[]> => {
  return invoke<LoanView[]>("get_statement_loans");
};

export const setStatementLoanFirstDue = async (
  key: string,
  firstDue: string | null,
): Promise<LoanView[]> => {
  return invoke<LoanView[]>("set_statement_loan_first_due", { key, firstDue });
};

/** Attach an imported loan paper to its loan account: an existing one, or a new one made empty and pending. */
export const attachStatementLoanDocument = async (
  documentId: string,
  choice: LoanAccountChoice,
): Promise<LoanView[]> => {
  return invoke<LoanView[]>("attach_statement_loan_document", { documentId, choice });
};

/** Validate a loan account as a whole: all its papers, in their order. */
export const validateStatementLoan = async (key: string): Promise<LoanView[]> => {
  return invoke<LoanView[]>("validate_statement_loan", { key });
};

/** Tell what the papers cannot say: the account debited, no early repayment, instalments never changed. */
export const setStatementLoanAnswers = async (
  key: string,
  answers: LoanAnswers,
): Promise<LoanView[]> => {
  return invoke<LoanView[]>("set_statement_loan_answers", { key, answers });
};

/** Attach a loan to the asset it finances (existing or new), or detach it with an empty link. */
export async function linkStatementLoanAsset(
  key: string,
  link: LoanAssetLink,
): Promise<LoanAssetLinked> {
  return invoke<LoanAssetLinked>("link_statement_loan_asset", { key, link });
}

/** Create or bring up to date the liability (Passifs) of each loan whose papers hold together. */
export async function syncStatementLoanLiabilities(): Promise<string[]> {
  return invoke<string[]>("sync_statement_loan_liabilities");
}
