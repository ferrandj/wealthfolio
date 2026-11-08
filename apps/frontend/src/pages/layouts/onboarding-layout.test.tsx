import { render } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { expect, it } from "vitest";
import { OnboardingLayout } from "./onboarding-layout";

it("sizes itself to the visible area of a phone browser, not to 100vh", () => {
  const { container } = render(
    <MemoryRouter>
      <OnboardingLayout />
    </MemoryRouter>,
  );
  // With only h-screen, the sticky footer (Continue) sits below the visible area while a
  // mobile browser's toolbars are up, and overflow-hidden leaves no way to scroll to it.
  expect(container.firstElementChild).toHaveClass("h-screen", "supports-[height:100dvh]:h-dvh");
});
