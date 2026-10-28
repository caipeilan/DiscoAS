import { memo } from "react";
import type { HandCard } from "../../types";
import { Cover } from "../../components/Cover";

/** The card face is shared by the hand surface and the transient drag surface. */
export const HandCardContent = memo(function HandCardContent({ card }: { card: HandCard }) {
  return <>
    <div className="hand-art">
      <Cover url={card.song.albumPicUrl} data={card.song.coverDataUri} prepared mystery={card.song.mysteryMode} label={card.song.title} />
      {!card.discovery && <span className="hand-auto-next-label" aria-hidden="true">AUTO NEXT</span>}
    </div>
    <strong>{card.song.mysteryMode ? "???" : card.song.title}</strong>
    <span>{card.song.mysteryMode ? "???" : card.song.artistNames.join(" / ")}</span>
  </>;
});
