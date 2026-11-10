// Regulated savings rates, tax table and the rate of each account
import type { RateInput, RateView } from "@/lib/types/rates";

import { invoke } from "./platform";

export const listRates = async (): Promise<RateView[]> => {
  return invoke<RateView[]>("list_rates");
};

export const saveRate = async (id: string, input: RateInput): Promise<RateView> => {
  return invoke<RateView>("save_rate", { id, input });
};

export const deleteRate = async (id: string): Promise<void> => {
  return invoke<void>("delete_rate", { id });
};
