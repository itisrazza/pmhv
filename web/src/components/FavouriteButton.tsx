import { Button } from "@swopstar/react-ui";
import { Star } from "lucide-react";
import { useFavourites } from "../lib/favourites";

export function FavouriteButton({ machineId }: { machineId: string }) {
  const { isFavourite, toggle } = useFavourites();
  const favourite = isFavourite(machineId);
  return (
    <Button
      size="icon"
      title={favourite ? "Remove from favourites" : "Add to favourites"}
      onClick={() => toggle(machineId)}
    >
      <Star className={favourite ? "fill-current" : undefined} />
    </Button>
  );
}
