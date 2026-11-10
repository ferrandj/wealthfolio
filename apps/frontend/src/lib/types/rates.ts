export type RateKind = "regulated" | "tax" | "account";

export interface RateView {
  id: string;
  kind: RateKind;
  label: string;
  /** Percent, e.g. 1.7 for 1.7 %. */
  rate: number | string;
  gross: boolean;
  taxExempt: boolean;
  /** Account rates: the tax-table row (`tax.type.*`) of the kind of investment; null = flat tax. */
  taxType: string | null;
  accountId: string | null;
  source: string | null;
  updatedAt: string;
  grossRate: number | string;
  netRate: number | string;
}

export interface RateInput {
  label?: string;
  rate?: number | string;
  gross?: boolean;
  taxExempt?: boolean;
  taxType?: string;
  source?: string;
  accountId?: string;
}

export interface RatesReview {
  /** The rates must be confirmed again: they are revised on 1 February and 1 August. */
  due: boolean;
  lastRevision: string;
  nextRevision: string;
  confirmedAt: string | null;
}
