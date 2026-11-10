export type RateKind = "regulated" | "tax" | "account";

export interface RateView {
  id: string;
  kind: RateKind;
  label: string;
  /** Percent, e.g. 1.7 for 1.7 %. */
  rate: number | string;
  gross: boolean;
  taxExempt: boolean;
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
  source?: string;
  accountId?: string;
}
