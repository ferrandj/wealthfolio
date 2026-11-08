import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { RESET_IDLE_MS, RESET_PRESSES, ResetProfileButton } from "./reset-profile-button";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function press(times: number) {
  for (let i = 0; i < times; i += 1) {
    fireEvent.click(screen.getByRole("button"));
  }
}

it("only confirms on the last of several presses in a row", () => {
  const onConfirm = vi.fn();
  render(<ResetProfileButton onConfirm={onConfirm} />);
  expect(screen.getByRole("button", { name: "Reset all data" })).toBeInTheDocument();

  press(RESET_PRESSES - 1);
  expect(onConfirm).not.toHaveBeenCalled();
  expect(screen.getByRole("button")).toHaveTextContent(
    `Press again to confirm (${RESET_PRESSES - 1}/${RESET_PRESSES})`,
  );

  press(1);
  expect(onConfirm).toHaveBeenCalledTimes(1);
  expect(screen.getByRole("button", { name: "Reset all data" })).toBeInTheDocument();
});

it("starts over when the presses stop for a while", () => {
  const onConfirm = vi.fn();
  render(<ResetProfileButton onConfirm={onConfirm} />);

  press(RESET_PRESSES - 1);
  act(() => {
    vi.advanceTimersByTime(RESET_IDLE_MS);
  });
  expect(screen.getByRole("button", { name: "Reset all data" })).toBeInTheDocument();

  press(1);
  expect(onConfirm).not.toHaveBeenCalled();
});

it("does nothing while disabled", () => {
  const onConfirm = vi.fn();
  render(<ResetProfileButton disabled onConfirm={onConfirm} />);
  press(RESET_PRESSES);
  expect(onConfirm).not.toHaveBeenCalled();
});
