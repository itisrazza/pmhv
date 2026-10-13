import { useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  getGetPreferencesQueryKey,
  useGetPreferences,
  useSetPreferences,
} from "../api/preferences/preferences";
import type { Preferences } from "../api/schemas";

export function useFavourites() {
  const preferences = useGetPreferences();
  const queryClient = useQueryClient();
  const save = useSetPreferences({
    mutation: {
      onSuccess: (saved) =>
        queryClient.setQueryData(getGetPreferencesQueryKey(), saved),
      onError: (error) => toast.error(error.message),
    },
  });
  const favourites = preferences.data?.favourites ?? [];

  const toggle = (id: string) => {
    const current: Preferences = preferences.data ?? { favourites: [] };
    const list = current.favourites ?? [];
    const next = list.includes(id)
      ? list.filter((favourite) => favourite !== id)
      : [...list, id];
    save.mutate({ data: { ...current, favourites: next } });
  };

  return {
    favourites,
    isFavourite: (id: string) => favourites.includes(id),
    toggle,
  };
}
