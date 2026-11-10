// Bank statements & account documents (mirrors wealthfolio-core `statements`).

export interface AccountIdentity {
  accountId: string;
  institution?: string | null;
  holderName?: string | null;
  holderAddress?: string | null;
  iban?: string | null;
  bic?: string | null;
  bankCode?: string | null;
  branchCode?: string | null;
  accountNumber?: string | null;
  ribKey?: string | null;
  editedByUser: boolean;
  updatedAt: string;
}

/** A missing field is left as it is; an empty string clears it. */
export interface AccountIdentityInput {
  institution?: string | null;
  holderName?: string | null;
  holderAddress?: string | null;
  iban?: string | null;
  bic?: string | null;
  bankCode?: string | null;
  branchCode?: string | null;
  accountNumber?: string | null;
  ribKey?: string | null;
}

export interface StatementDocument {
  id: string;
  accountId?: string | null;
  fileName: string;
  storedName: string;
  contentType: string;
  sizeBytes: number;
  sha256: string;
  institution?: string | null;
  adapterId?: string | null;
  documentKind?: string | null;
  accountType?: string | null;
  periodStart?: string | null;
  periodEnd?: string | null;
  holderName?: string | null;
  iban?: string | null;
  readerVersion?: string | null;
  importedCount: number;
  duplicateCount: number;
  warnings: string[];
  importRunId?: string | null;
  status: "IMPORTED" | "STORED";
  createdAt: string;
}

export interface StatementDocumentContent {
  document: StatementDocument;
  contentBase64: string;
}

export interface StatementCandidate {
  adapterId: string;
  label: string;
  institution?: string | null;
  kind: string;
  score: number;
  reasons: string[];
  generic: boolean;
}

export interface StatementTransactionPreview {
  date: string;
  valueDate?: string | null;
  label: string;
  amount: string;
  currency: string;
  kind?: string | null;
  counterparty?: string | null;
  counterpartyIban?: string | null;
  reference?: string | null;
  memo?: string | null;
}

export interface StatementPreview {
  index: number;
  institution?: string | null;
  adapterId?: string | null;
  documentKind?: string | null;
  accountLabel?: string | null;
  accountType?: string | null;
  currency: string;
  holderName?: string | null;
  holderAddress?: string | null;
  iban?: string | null;
  bic?: string | null;
  bankCode?: string | null;
  branchCode?: string | null;
  accountNumber?: string | null;
  ribKey?: string | null;
  balance?: string | null;
  balanceDate?: string | null;
  openingBalance?: string | null;
  openingDate?: string | null;
  periodStart?: string | null;
  periodEnd?: string | null;
  transactions: StatementTransactionPreview[];
  holdingsCount: number;
  warnings: string[];
  confidence: string;
}

export interface FileAnalysis {
  fileName: string;
  sizeBytes: number;
  sha256: string;
  format: string;
  institution?: string | null;
  /** auto | ask | fallback | none | chosen */
  decision: string;
  message: string;
  candidates: StatementCandidate[];
  chosenAdapterId?: string | null;
  readerVersion: string;
  error?: string | null;
}

export interface MatchedAccount {
  id: string;
  name: string;
  currency: string;
  matchedOn: "iban" | "accountNumber";
}

export interface AnalyzedStatement {
  preview: StatementPreview;
  mode: "transactions" | "documentOnly";
  matchedAccount?: MatchedAccount | null;
  suggestedAccountName: string;
  suggestedAccountType: string;
  knownDuplicates: number;
}

export interface StatementAnalysis {
  analysis: FileAnalysis;
  statements: AnalyzedStatement[];
  existingDocument?: StatementDocument | null;
}

export interface StatementOverride {
  index: number;
  accountId?: string | null;
  newAccountName?: string | null;
  skip?: boolean;
  identity?: AccountIdentityInput | null;
}

export interface StatementImportRequest {
  fileName: string;
  contentBase64: string;
  adapterId?: string | null;
  overrides?: StatementOverride[];
  allowDuplicateDocument?: boolean;
}

export interface StatementImportOutcome {
  index: number;
  accountId?: string | null;
  accountName?: string | null;
  createdAccount: boolean;
  imported: number;
  duplicates: number;
  skipped: number;
  openingBalanceAdded: boolean;
  messages: string[];
}

export interface StatementImportResult {
  document?: StatementDocument | null;
  alreadyImported: boolean;
  outcomes: StatementImportOutcome[];
}

export interface SupportedStatementAdapter {
  id: string;
  label: string;
  kind: string;
  description: string;
  formats: string[];
  generic: boolean;
}

export interface SupportedInstitution {
  institution: string;
  adapters: SupportedStatementAdapter[];
}
