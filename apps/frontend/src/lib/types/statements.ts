// Bank statements & account documents (mirrors wealthfolio-core `statements`).

export interface AccountIdentity {
  accountId: string;
  institution?: string | null;
  holderName?: string | null;
  /** The people the account belongs to, read from the documents (several for a joint account). */
  holders?: string[];
  holderAddress?: string | null;
  /** The address cut into parts (read-only view of `holderAddress`). */
  holderStreet?: string | null;
  holderPostalCode?: string | null;
  holderCity?: string | null;
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
  /** The address in parts: a part given replaces the same part, the others stay. */
  holderStreet?: string | null;
  holderPostalCode?: string | null;
  holderCity?: string | null;
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
  /** STORED: kept without operations. PENDING: operations imported, waiting for review. VALIDATED: checked. */
  status: "STORED" | "PENDING" | "VALIDATED";
  createdAt: string;
  /** JSON of a {@link StatementReviewReason}: why the document waits for review. */
  reviewReason?: string | null;
  toReviewCount: number;
  validatedAt?: string | null;
}

/** An operation of the imported statements to look at again. */
export interface OperationToReview {
  documentId: string;
  fileName: string;
  accountName?: string | null;
  currency: string;
  /** doubt | to_categorize | loan_proposed | loan_to_attach */
  why: string[];
  operation: ReviewedOperation;
}

export interface StatementReviewReason {
  /** Operations whose category is a low-confidence guess. */
  doubts: number;
  /** Closing balance minus (opening balance + operations), when it is not zero. */
  balanceGap?: string | null;
  warnings: number;
  /** It does not follow the previous statement of its account (missing days, overlap, balance). */
  chain?: boolean;
  /** A statement next to it came or went after it was validated: to check again. */
  reopened?: boolean;
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
  categoryId?: string | null;
  categoryTaxonomyId?: string | null;
  /** high | medium | low */
  categoryConfidence: "high" | "medium" | "low";
  categoryReason?: string | null;
}

export interface StatementPreview {
  index: number;
  institution?: string | null;
  adapterId?: string | null;
  documentKind?: string | null;
  accountLabel?: string | null;
  accountType?: string | null;
  currency: string;
  /** The people the account belongs to (several for a joint account). */
  holders?: string[];
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
  /** Stored statements of the same account whose period overlaps this one (another file). */
  overlaps: StatementOverlap[];
}

export interface StatementOverlap {
  documentId: string;
  fileName: string;
  periodStart: string | null;
  periodEnd: string | null;
  status: string;
  importedCount: number;
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
  /** Stored statements this one replaces (chosen after an overlap was shown). */
  replaceDocumentIds?: string[];
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
  categorized: number;
  toReview: number;
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

export interface ReviewedOperation {
  row: number;
  activityId?: string | null;
  date: string;
  label: string;
  /** Signed: negative for money going out. */
  amount: string;
  currency: string;
  kind?: string | null;
  counterparty?: string | null;
  taxonomyId?: string | null;
  categoryId?: string | null;
  /** import (a guess) | manual | rule */
  categorySource?: string | null;
  suggestedTaxonomyId?: string | null;
  suggestedCategoryId?: string | null;
  confidence: "high" | "medium" | "low";
  reason?: string | null;
  needsReview: boolean;
  /** The stored category is only the catch-all and the reader now suggests a precise one. */
  outdated?: boolean;
  /** Money moved between own accounts, but the other account is not one Wealthfolio knows. */
  otherAccountUnknown?: boolean;
  /** The loan this payment pays, when known. */
  loanKey?: string | null;
  loanLabel?: string | null;
  /** matched | proposed | to_attach */
  loanState?: "matched" | "proposed" | "to_attach" | null;
}

export interface StatementReview {
  index: number;
  accountId?: string | null;
  accountName?: string | null;
  currency: string;
  periodStart?: string | null;
  periodEnd?: string | null;
  openingBalance?: string | null;
  closingBalance?: string | null;
  computedBalance?: string | null;
  balanceGap?: string | null;
  operations: ReviewedOperation[];
}

export interface DocumentReview {
  document: StatementDocument;
  statements: StatementReview[];
  earlierPending: string[];
  warnings: string[];
  /** For a loan paper: the whole loan (terms, plan, checks). */
  loan?: LoanView;
  /** How the statement follows the previous one of its account and leads to the next one. */
  continuity?: StatementContinuity | null;
}

/** A statement next to another one of the same account. */
export interface StatementNeighbour {
  id: string;
  fileName: string;
  status: string;
  periodStart?: string | null;
  periodEnd?: string | null;
  openingBalance?: string | null;
  closingBalance?: string | null;
}

export interface StatementContinuity {
  previous?: StatementNeighbour | null;
  next?: StatementNeighbour | null;
  /** first | follows | gap | overlap */
  state: "first" | "follows" | "gap" | "overlap";
  missingFrom?: string | null;
  missingTo?: string | null;
  balanceFollows?: boolean | null;
  balanceDifference?: string | null;
  nextState?: "follows" | "gap" | "overlap" | null;
  nextBalanceFollows?: boolean | null;
  earlierPending: number;
}

export interface ReviewDecision {
  activityId: string;
  taxonomyId: string;
  categoryId: string;
  /** Carry the category to the other operations with an equivalent label. */
  remember?: boolean;
}

export interface RevalidationOutcome {
  documentId: string;
  fileName: string;
  status: StatementDocument["status"];
  toReviewCount: number;
  reviewReason?: string | null;
  error?: string | null;
}

export interface RevalidationResult {
  total: number;
  validated: number;
  pending: number;
  failed: number;
  outcomes: RevalidationOutcome[];
}

export interface MissingMonth {
  month: string;
  normal: boolean;
}

export interface BalanceBreak {
  previousDocumentId: string;
  nextDocumentId: string;
  expected: string;
  found: string;
}

export interface AccountCoverage {
  accountId: string;
  firstMonth: string | null;
  lastMonth: string | null;
  missingMonths: MissingMonth[];
  balanceBreaks: BalanceBreak[];
}

export interface OutdatedDocuments {
  currentVersion: string;
  documentIds: string[];
}

export interface LoanPlanRow {
  kind: "installment" | "prepayment";
  date: string | null;
  number: number | null;
  capital: number | string;
  interest: number | string;
  insurance: number | string;
  total: number | string;
  remaining: number | string;
  source: string;
  status: "paid" | "next" | "future";
  note: string;
}

export interface LoanPlanCheck {
  status: "ok" | "warn" | "info";
  text: string;
}

export interface LoanSaved {
  interest: number | string;
  insurance: number | string;
  months: number;
}

export interface LoanPlanSummary {
  asOf: string;
  owed: number | string | null;
  endDate: string | null;
  paidCount: number;
  remainingCount: number;
  totalCount: number;
  next: LoanPlanRow | null;
  dated: boolean;
  futureInterest: number | string;
  futureInsurance: number | string;
  futureTotal: number | string;
  paidTotal: number | string;
  paidInterest: number | string;
  lifetimeInterest: number | string;
  repaidEarly: number | string;
  saved: LoanSaved | null;
}

export interface LoanPlan {
  rows: LoanPlanRow[];
  checks: LoanPlanCheck[];
  summary: LoanPlanSummary | null;
}

export interface LoanDocumentRef {
  documentId: string;
  kind: string;
  fileName: string;
  status: string;
}

/** Where an imported loan paper goes: an existing loan account, or a new one (empty, pending). */
export interface LoanAccountChoice {
  liabilityId?: string | null;
  newAccount?: { name: string; key?: string | null } | null;
}

/** What the person told about a loan, when the papers could not. */
export interface LoanAnswers {
  debitAccountId?: string | null;
  noEarlyRepayment?: boolean;
  paymentsUnchanged?: boolean;
  /** Debits confirmed for an instalment although their amount differs (a first instalment pro rata). */
  confirmedPayments?: { date: string; activityId: string }[];
  /** Debits detached from the loan: never matched to it again. */
  detachedActivities?: string[];
}

/** Something the loan still lacks, and the papers where it can be found. */
export interface LoanGap {
  code: string;
  text: string;
  /** loan_offer | loan_event | loan_schedule | statement */
  foundIn: string[];
  blocking: boolean;
  /** first_due | debit_account | no_early_repayment | payments_unchanged | attach_account */
  answer?: string | null;
}

/** One instalment (or early repayment) already due, and the debit that pays it. */
export interface LoanPayment {
  row: number;
  number?: number | null;
  date: string;
  kind: string;
  expected: number | string;
  /** found | proposed | missing | uncovered | unknown */
  state: "found" | "proposed" | "missing" | "uncovered" | "unknown";
  /** The person confirmed this debit although its amount differs. */
  confirmed?: boolean;
  /** Why a debit of another amount is the instalment (a first instalment pro rata). */
  note?: string | null;
  activityId?: string | null;
  accountId?: string | null;
  paidOn?: string | null;
  amount?: number | string | null;
}

export interface LoanPayments {
  accountId?: string | null;
  accountSource?: "answer" | "statements" | null;
  expected: number;
  found: number;
  missing: number;
  uncovered: number;
  proposed?: number;
  lines: LoanPayment[];
}

export interface LoanView {
  key: string;
  label: string;
  borrower: string | null;
  product: string | null;
  principal: number | string | null;
  duration: number | null;
  rate: number | string | null;
  taeg: number | string | null;
  documents: LoanDocumentRef[];
  firstDue: string | null;
  inferredFirstDue: string | null;
  plan: LoanPlan;
  /** The liability (Passifs) kept for this loan, once its papers hold together. */
  liabilityId?: string | null;
  /** The asset (house, car…) this loan finances. */
  linkedAssetId?: string | null;
  /** The loan account (liability) the papers are attached to. */
  liabilityName?: string | null;
  /** The loan account exists but waits for its papers to be validated (it stays empty). */
  liabilityPending?: boolean;
  /** Where instalment n°1 comes from: papers | statements | typed. */
  firstDueSource?: "papers" | "statements" | "typed" | null;
  answers?: LoanAnswers;
  /** What is still missing, with the papers where it can be found. */
  gaps?: LoanGap[];
  /** Each instalment already due, followed in the bank accounts. */
  payments?: LoanPayments;
}

/** An asset created with the link: a property, a vehicle or another asset. */
export interface NewLoanAsset {
  kind: "PROPERTY" | "VEHICLE" | "OTHER";
  name: string;
  street?: string | null;
  postalCode?: string | null;
  city?: string | null;
  value: number;
  valueDate?: string | null;
  purchasePrice?: number | null;
  purchaseDate?: string | null;
}

/** What a loan finances: an existing asset, a new one, or neither (removes the link). */
export interface LoanAssetLink {
  assetId?: string | null;
  newAsset?: NewLoanAsset | null;
}

export interface LoanAssetLinked {
  liabilityId: string;
  linkedAssetId: string | null;
  createdAsset: boolean;
}
