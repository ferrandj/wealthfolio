import { Separator } from "@wealthfolio/ui/components/ui/separator";
import { useTranslation } from "react-i18next";
import { usePlatform } from "@/hooks/use-platform";
import { ResetProfileButton } from "@/features/profiles/reset-profile-button";
import { useProfile } from "@/features/profiles/profile-context";
import { SettingsHeader } from "../settings-header";
import { AutoUpdateSettings } from "./auto-update-settings";
import { BaseCurrencySettings } from "./currency-settings";
import { DatabaseEncryptionSettings } from "./database-encryption-settings";
import { ExchangeRatesSettings } from "./exchange-rates/exchange-rates-settings";
import { LanguageRegionSettings } from "./language-region-settings";

export default function GeneralSettingsPage() {
  const { t } = useTranslation();
  const { isMobile } = usePlatform();
  const { t: tc } = useTranslation("common");
  const profile = useProfile();

  return (
    <div className="space-y-6">
      <SettingsHeader
        heading={t("settings:nav.items.general")}
        text={t("settings:general_page_description")}
      />
      <Separator />
      <BaseCurrencySettings />
      <LanguageRegionSettings />
      <div className="pt-6">
        <ExchangeRatesSettings />
      </div>
      <div className="pt-6">
        <DatabaseEncryptionSettings />
      </div>
      {!isMobile && (
        <div className="pt-6">
          <AutoUpdateSettings />
        </div>
      )}
      {profile?.profile && profile.resetProfile && (
        <section className="border-destructive/25 space-y-2 border-t pt-6">
          <h2 className="font-medium">{tc("profiles.resetDataTitle")}</h2>
          <p className="text-muted-foreground text-sm">
            {tc(
              profile.profile.lockEnabled
                ? "profiles.resetDataProtected"
                : "profiles.resetDataHelp",
            )}
          </p>
          {/* Deleting a protected profile asks for its password, so only unprotected ones reset here. */}
          {!profile.profile.lockEnabled && <ResetProfileButton onConfirm={profile.resetProfile} />}
        </section>
      )}
    </div>
  );
}
