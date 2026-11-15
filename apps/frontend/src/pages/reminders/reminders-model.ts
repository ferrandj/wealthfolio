import type { Reminder } from "@/lib/types/reminders";

/** The hours a reminder can be set at; they are the hours reminders are looked at. */
export const REMINDER_HOURS = [10, 15, 20] as const;
export const DEFAULT_REMINDER_HOUR = 10;

/** `YYYY-MM-DD` of a local date. */
export function dayKey(date: Date): string {
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${date.getFullYear()}-${month}-${day}`;
}

/** The day of a reminder (`YYYY-MM-DD`): its date as written, whatever the browser's time zone. */
export const reminderDay = (reminder: Reminder): string => reminder.dueAt.slice(0, 10);

/** The hour of a reminder ("10:00"). */
export const reminderHour = (reminder: Reminder): string => reminder.dueAt.slice(11, 16);

/** A reminder's day and hour as a local date, to write it out in words. */
export function reminderDate(reminder: Reminder): Date {
  const [year, month, day] = reminderDay(reminder).split("-").map(Number);
  const [hour, minute] = reminderHour(reminder).split(":").map(Number);
  return new Date(year, month - 1, day, hour || 0, minute || 0);
}

/** The reminders of each day. */
export function byDay(reminders: Reminder[]): Map<string, Reminder[]> {
  const days = new Map<string, Reminder[]>();
  for (const reminder of reminders) {
    const key = reminderDay(reminder);
    days.set(key, [...(days.get(key) ?? []), reminder]);
  }
  return days;
}

/**
 * The weeks a month is shown in: full weeks from Monday (or Sunday when `weekStartsOn` is 0), with the days of
 * the neighbouring months that fill them.
 */
export function monthWeeks(year: number, month: number, weekStartsOn: 0 | 1 = 1): Date[][] {
  const first = new Date(year, month, 1);
  const lead = (first.getDay() - weekStartsOn + 7) % 7;
  const days = new Date(year, month + 1, 0).getDate();
  const weeks: Date[][] = [];
  for (let offset = -lead; offset < days; offset += 7) {
    weeks.push(Array.from({ length: 7 }, (_, i) => new Date(year, month, 1 + offset + i)));
  }
  return weeks;
}

/** The next of the hours reminders are looked at, strictly after `now`. */
export function nextCheck(now: Date = new Date()): Date {
  for (const hour of REMINDER_HOURS) {
    const at = new Date(now.getFullYear(), now.getMonth(), now.getDate(), hour, 0, 0, 0);
    if (at.getTime() > now.getTime()) return at;
  }
  return new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, REMINDER_HOURS[0], 0, 0, 0);
}

/** What a reminder was told under: itself on that day and hour. */
export const toldKey = (reminder: Reminder): string => `${reminder.id}@${reminder.dueAt}`;

/** Among the due reminders, those not told in the app yet. */
export function newlyDue(reminders: Reminder[], told: ReadonlySet<string>): Reminder[] {
  return reminders.filter((r) => r.due && !told.has(toldKey(r)));
}
