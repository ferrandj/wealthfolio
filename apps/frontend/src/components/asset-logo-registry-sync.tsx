import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";

import { findMissingAssetLogos, useAssetLogoIndex } from "@/hooks/use-asset-logos";
import { assetLogoRegistry } from "@/lib/asset-logo-registry";

/**
 * Feeds the custom-logo index from react-query into the module registry that
 * `TickerAvatar` reads synchronously. Renders nothing. Mount once, inside the
 * authenticated tree (the index lists the user's assets).
 */
export function AssetLogoRegistrySync() {
  const { data } = useAssetLogoIndex();
  const queryClient = useQueryClient();

  // Once per session: the assets still without a picture get one (their house's, else their bank's). Nothing
  // is fetched when none is missing.
  useEffect(() => {
    void findMissingAssetLogos(queryClient);
  }, [queryClient]);

  useEffect(() => {
    if (data) assetLogoRegistry.setIndex(data);
  }, [data]);

  useEffect(() => () => assetLogoRegistry.reset(), []);

  return null;
}
