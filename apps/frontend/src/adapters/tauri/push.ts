// Notifications outside the app are sent by the web server: the desktop app has none to subscribe to.
import type {
  PushDelivery,
  PushStatus,
  PushSubscriptionInput,
  ReminderCheck,
} from "@/lib/types/reminders";

export const pushAvailable = false;

const unavailable = () =>
  Promise.reject(new Error("Notifications outside the app are sent by the web server."));

export const getPushStatus = (): Promise<PushStatus> => unavailable();

export const subscribePush = (_subscription: PushSubscriptionInput): Promise<PushStatus> =>
  unavailable();

export const unsubscribePush = (_endpoint: string): Promise<PushStatus> => unavailable();

export const sendTestPush = (): Promise<PushDelivery> => unavailable();

export const checkRemindersNow = (): Promise<ReminderCheck> => unavailable();
