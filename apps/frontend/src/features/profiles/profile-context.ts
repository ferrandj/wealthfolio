import { createContext, useContext } from "react";
import type { ProfileSummary } from "./api";

interface ProfileContextValue {
  profile: ProfileSummary | undefined;
  profileCount: number;
  profiles: ProfileSummary[];
  addProfile: () => void;
  manageProfile: () => void;
  lockProfile: () => void;
  switchProfile: () => void;
  selectProfile: (profileId: string) => void;
  /** Erase everything of the open profile and start again from the welcome screen. */
  resetProfile?: () => void;
}

export const ProfileContext = createContext<ProfileContextValue | null>(null);

export const useProfile = () => useContext(ProfileContext);
