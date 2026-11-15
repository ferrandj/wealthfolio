/** One thing to do, on its day and hour (in the person's time). */
export interface Reminder {
  /** `document:<source key>` | `rates`. */
  id: string;
  /** document: a document to bring back; rates: the rate tables to confirm. */
  kind: "document" | "rates";
  /** `YYYY-MM-DDTHH:mm:ss`, in the person's time. */
  dueAt: string;
  /** Its day and hour have come. */
  due: boolean;
  label: string;
  accountId: string | null;
  accountName: string | null;
  institution: string | null;
  howTo: string | null;
  /** A document: its source (`account id|reading id`). */
  sourceKey: string | null;
  everyDays: number | null;
}

/** What a browser needs to subscribe to notifications, and the devices that are. */
export interface PushStatus {
  publicKey: string;
  /** The push addresses of the devices subscribed. */
  endpoints: string[];
}

export interface PushDelivery {
  sent: number;
  failed: number;
  removed: number;
}

/** What a check of the reminders did. */
export interface ReminderCheck {
  reminders: Reminder[];
  delivery: PushDelivery;
}

/** A browser's push subscription, as the server keeps it. */
export interface PushSubscriptionInput {
  endpoint: string;
  p256dh: string;
  auth: string;
  label?: string | null;
}
