import { describe, expect, it } from "vitest";

import type { Reminder } from "@/lib/types/reminders";

import {
  byDay,
  dayKey,
  monthWeeks,
  newlyDue,
  nextCheck,
  reminderHour,
  toldKey,
} from "./reminders-model";

const reminder = (id: string, dueAt: string, due = false): Reminder => ({
  id,
  kind: "document",
  dueAt,
  due,
  label: id,
  accountId: null,
  accountName: null,
  institution: null,
  howTo: null,
  sourceKey: null,
  everyDays: null,
});

describe("reminders", () => {
  it("shows a month in full weeks from Monday", () => {
    // November 2026 starts on a Sunday and ends on a Monday
    const weeks = monthWeeks(2026, 10);
    expect(weeks).toHaveLength(6);
    expect(weeks.every((week) => week.length === 7)).toBe(true);
    expect(dayKey(weeks[0][0])).toBe("2026-10-26");
    expect(dayKey(weeks[0][6])).toBe("2026-11-01");
    expect(dayKey(weeks[5][0])).toBe("2026-11-30");
    // and from Sunday where weeks start on Sunday
    expect(dayKey(monthWeeks(2026, 10, 0)[0][0])).toBe("2026-11-01");
    // a month that starts on a Monday has no day of the month before
    expect(dayKey(monthWeeks(2027, 1)[0][0])).toBe("2027-02-01");
    expect(monthWeeks(2027, 1)).toHaveLength(4);
  });

  it("puts each reminder on its day, at its hour", () => {
    const days = byDay([
      reminder("a", "2026-11-20T10:00:00"),
      reminder("b", "2026-11-20T20:00:00"),
      reminder("c", "2026-12-01T15:00:00"),
    ]);
    expect(days.get("2026-11-20")?.map((r) => r.id)).toEqual(["a", "b"]);
    expect(days.get("2026-12-01")?.map(reminderHour)).toEqual(["15:00"]);
  });

  it("looks again at ten, three and eight", () => {
    const at = (hour: number, minute = 0) => new Date(2026, 10, 15, hour, minute);
    expect(nextCheck(at(7, 30)).getHours()).toBe(10);
    expect(nextCheck(at(10)).getHours()).toBe(15);
    expect(nextCheck(at(19, 59)).getHours()).toBe(20);
    const tomorrow = nextCheck(at(20));
    expect([tomorrow.getDate(), tomorrow.getHours()]).toEqual([16, 10]);
  });

  it("tells a due reminder once, and again when it comes due on another day", () => {
    const first = reminder("a", "2026-11-15T10:00:00", true);
    const later = reminder("b", "2026-11-20T10:00:00", false);
    const told = new Set<string>();
    expect(newlyDue([first, later], told).map((r) => r.id)).toEqual(["a"]);
    told.add(toldKey(first));
    expect(newlyDue([first, later], told)).toEqual([]);
    expect(newlyDue([reminder("a", "2026-11-22T10:00:00", true)], told)).toHaveLength(1);
  });
});
