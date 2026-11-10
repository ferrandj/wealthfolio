// Bank statements & account documents
import type {
  AccountCoverage,
  LoanView,
  OutdatedDocuments,
  AccountIdentity,
  AccountIdentityInput,
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
