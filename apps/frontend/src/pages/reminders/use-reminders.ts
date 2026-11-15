import { getReminders } from "@/adapters";
import { QueryKeys } from "@/lib/query-keys";
import { useQuery } from "@tanstack/react-query";

/** The things to do, each on its day and hour, the earliest first. */
export function useReminders() {
  return useQuery({
    queryKey: [QueryKeys.REMINDERS],
    queryFn: getReminders,
    staleTime: 60_000,
  });
}
