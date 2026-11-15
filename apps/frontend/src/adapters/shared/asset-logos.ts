// Asset Logo Commands
import type {
  AssetLogo,
  AssetLogoSummary,
  FindAssetLogosReport,
  FindAssetLogosRequest,
  UpsertAssetLogoInput,
} from "@/lib/types";

import { invoke } from "./platform";

/**
 * Find a picture for the assets that have none (the fund's house, else the bank that holds it). The backend
 * fetches each picture once, shrinks it and stores it as the asset's logo.
 */
export const findAssetLogos = async (
  request: FindAssetLogosRequest = {},
): Promise<FindAssetLogosReport> => {
  return invoke<FindAssetLogosReport>("find_asset_logos", { request });
};

/**
 * List every asset with a custom logo override (metadata only, no image bytes).
 */
export const listAssetLogos = async (): Promise<AssetLogoSummary[]> => {
  return invoke<AssetLogoSummary[]>("list_asset_logos", {});
};

/**
 * Get the custom logo (with image bytes) for one asset, or null when none is set.
 */
export const getAssetLogo = async (assetId: string): Promise<AssetLogo | null> => {
  const logo = await invoke<AssetLogo | null | undefined>("get_asset_logo", { assetId });
  return logo ?? null;
};

/**
 * Create or replace the custom logo for an asset. `dataBase64` must be a PNG.
 */
export const upsertAssetLogo = async (
  assetId: string,
  payload: UpsertAssetLogoInput,
): Promise<AssetLogo> => {
  return invoke<AssetLogo>("upsert_asset_logo", { assetId, payload });
};

/**
 * Remove the custom logo for an asset (falls back to the bundled logo/initials).
 */
export const deleteAssetLogo = async (assetId: string): Promise<void> => {
  return invoke<void>("delete_asset_logo", { assetId });
};
