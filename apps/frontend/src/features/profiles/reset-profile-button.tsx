import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@wealthfolio/ui";

export const RESET_PRESSES = 5;
// Presses only count while they come one after the other; stopping resets the count.
export const RESET_IDLE_MS = 5000;

interface ResetProfileButtonProps {
  disabled?: boolean;
  onConfirm: () => void;
}

/** Runs a destructive action only after several presses in a row, so a stray click cannot erase data. */
export function ResetProfileButton({ disabled, onConfirm }: ResetProfileButtonProps) {
  const { t } = useTranslation("common");
  const [pressed, setPressed] = useState(0);
  const idleTimer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(idleTimer.current), []);

  function press() {
    window.clearTimeout(idleTimer.current);
    const next = pressed + 1;
    if (next >= RESET_PRESSES) {
      setPressed(0);
      onConfirm();
      return;
    }
    setPressed(next);
    idleTimer.current = window.setTimeout(() => setPressed(0), RESET_IDLE_MS);
  }

  return (
    <Button type="button" variant="destructive" disabled={disabled} onClick={press}>
      {pressed === 0
        ? t("profiles.resetDataButton")
        : t("profiles.resetDataProgress", { pressed, total: RESET_PRESSES })}
    </Button>
  );
}
