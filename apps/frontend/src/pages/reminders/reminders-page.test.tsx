import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { Reminder } from "@/lib/types/reminders";

import RemindersPage from "./reminders-page";
import { dayKey } from "./reminders-model";

const mocks = vi.hoisted(() => ({
  reminders: [] as unknown[],
  navigate: vi.fn(),
  snooze: vi.fn((_keys: string[], _until: string) => Promise.resolve([])),
}));

vi.mock("@/adapters", () => ({
  pushAvailable: false,
  getReminders: () => Promise.resolve(mocks.reminders),
  getStatementDocumentSources: () => Promise.resolve([]),
  getPushStatus: vi.fn(),
  subscribePush: vi.fn(),
  unsubscribePush: vi.fn(),
  sendTestPush: vi.fn(),
  checkRemindersNow: vi.fn(),
  snoozeStatementDocumentSources: mocks.snooze,
  setStatementDocumentSource: vi.fn(),
}));
vi.mock("@/hooks/use-accounts", () => ({ useAccounts: () => ({ accounts: [] }) }));
vi.mock("react-router-dom", async (original) => ({
  ...(await original<typeof import("react-router-dom")>()),
  useNavigate: () => mocks.navigate,
}));
vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { count?: number }) =>
      options?.count === undefined ? key : `${key}:${options.count}`,
    i18n: { language: "fr" },
  }),
}));

const now = new Date();
const at = (offsetDays: number, hour: number) => {
  const day = new Date(now.getFullYear(), now.getMonth(), 15 + offsetDays);
  return `${dayKey(day)}T${String(hour).padStart(2, "0")}:00:00`;
};
const reminder = (over: Partial<Reminder>): Reminder => ({
  id: "document:a|x",
  kind: "document",
  dueAt: at(0, 10),
  due: false,
  label: "Banque · relevé (PDF)",
  accountId: "a",
  accountName: "Compte courant",
  institution: null,
  howTo: "Espace client > Relevés",
  sourceKey: "a|x",
  everyDays: 30,
  ...over,
});

function renderPage() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <RemindersPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("RemindersPage", () => {
  beforeEach(() => {
    mocks.navigate.mockReset();
    mocks.snooze.mockClear();
    mocks.reminders = [
      reminder({ id: "document:a|x", dueAt: at(0, 10), due: true }),
      reminder({ id: "document:b|y", dueAt: at(0, 20), label: "Assurance · positions (CSV)" }),
      reminder({ id: "rates", kind: "rates", dueAt: at(3, 10), label: "", sourceKey: null }),
    ];
  });

  it("puts each reminder on its day of the calendar and lists them by date with their hour", async () => {
    renderPage();
    const list = await screen.findByTestId("reminder-list");
    const rows = within(list).getAllByRole("listitem");
    expect(rows).toHaveLength(3);
    // the earliest first, each with its exact day and hour
    expect(rows[0]).toHaveTextContent("Banque · relevé (PDF)");
    expect(rows[0]).toHaveTextContent("10:00");
    expect(rows[0]).toHaveTextContent("common:reminders.due_now");
    expect(rows[0]).toHaveTextContent("Espace client > Relevés");
    expect(rows[1]).toHaveTextContent("20:00");
    expect(rows[2]).toHaveTextContent("common:reminders.rates_title");

    const calendar = screen.getByTestId("reminder-calendar");
    const day = calendar.querySelector(`[data-day="${at(0, 10).slice(0, 10)}"]`)!;
    expect(day).toHaveTextContent("2");
    expect(calendar.querySelector(`[data-day="${at(3, 10).slice(0, 10)}"]`)).toHaveTextContent("1");
    expect(calendar.querySelector(`[data-day="${at(1, 10).slice(0, 10)}"]`)).toHaveTextContent(
      /^\d+$/,
    );
  });

  it("shows only a day's reminders when that day is picked, and all again when it is picked again", async () => {
    renderPage();
    await screen.findByTestId("reminder-list");
    const day = screen
      .getByTestId("reminder-calendar")
      .querySelector(`[data-day="${at(3, 10).slice(0, 10)}"]`)!;
    fireEvent.click(day);
    expect(within(screen.getByTestId("reminder-list")).getAllByRole("listitem")).toHaveLength(1);
    expect(day).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(day);
    expect(within(screen.getByTestId("reminder-list")).getAllByRole("listitem")).toHaveLength(3);
  });

  it("leads to where each thing is done, and puts off what is due", async () => {
    renderPage();
    const rows = within(await screen.findByTestId("reminder-list")).getAllByRole("listitem");
    fireEvent.click(within(rows[0]).getByText("common:reminders.drop"));
    expect(mocks.navigate).toHaveBeenCalledWith("/settings/documents?account=a");
    fireEvent.click(within(rows[2]).getByText("common:reminders.open_rates"));
    expect(mocks.navigate).toHaveBeenCalledWith("/settings/rates");
    // only what is due can be put off
    expect(within(rows[1]).queryByText("common:reminders.later:7")).toBeNull();
    fireEvent.click(within(rows[0]).getByText("common:reminders.later:7"));
    await vi.waitFor(() => expect(mocks.snooze).toHaveBeenCalled());
    expect(mocks.snooze.mock.calls[0][0]).toEqual(["a|x"]);
  });

  it("says where notifications come from when this is not the web app", async () => {
    renderPage();
    expect(await screen.findByTestId("push-unavailable")).toHaveTextContent(
      "common:reminders.push.unavailable_desktop",
    );
  });
});
