// Notifications on the person's browsers and phones (web push). The server sends them; a browser subscribes.
import type {
  PushDelivery,
  PushStatus,
  PushSubscriptionInput,
  ReminderCheck,
} from "@/lib/types/reminders";

import { invoke } from "./core";

/** Notifications outside the app need the server: they exist on the web app. */
export const pushAvailable = true;

export const getPushStatus = async (): Promise<PushStatus> => {
  return invoke<PushStatus>("get_push_status");
};

export const subscribePush = async (subscription: PushSubscriptionInput): Promise<PushStatus> => {
  return invoke<PushStatus>("subscribe_push", { subscription });
};

export const unsubscribePush = async (endpoint: string): Promise<PushStatus> => {
  return invoke<PushStatus>("unsubscribe_push", { endpoint });
};

/** Send a notification to every device now, to see that it arrives. */
export const sendTestPush = async (): Promise<PushDelivery> => {
  return invoke<PushDelivery>("send_test_push");
};

/** Look at the reminders now instead of at the next of the three daily checks. */
export const checkRemindersNow = async (): Promise<ReminderCheck> => {
  return invoke<ReminderCheck>("check_reminders");
};
