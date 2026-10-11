import { memo, type CSSProperties } from "react";
import type { HandCard } from "../../types";
import { desktop } from "../../services/desktop";
import { t } from "../../i18n";
import { Cover } from "../../components/Cover";
import { Icon } from "../../Icon";
import { HandDock } from "./HandDock";
import { useHandController } from "./useHandController";
import "../../App.css";
import "./hand.css";
const HandCardContent = memo(function HandCardContent({ card }: { card: HandCard }) {
  return <>
    <div className="hand-art"><Cover url={card.song.albumPicUrl} data={card.song.coverDataUri} prepared mystery={card.song.mysteryMode} label={card.song.name} /></div>
    <strong>{card.song.mysteryMode ? "???" : card.song.name}</strong>
    <span>{card.song.mysteryMode ? "???" : card.song.artistNames.join(" / ")}</span>
  </>;
});

/** Rendering delegates persisted operations, pointer input, motion and native regions to their owners. */
export function HandWindow() {
  const { snapshot, phase, hidden, inputMode, cardsMounted, stage, surface, cardSize, toolbarElement, dockPosition, dockAreas, diameter,
    busy, error, clearConfirm, toCanvas, setDockMoving, moveDock, toggleDock, clearDock, visibleCards, discardById,
    flying, drag, poses, cardIndices, active, operation, finishFlight, onCardPointerDown, onCardPointerMove,
    endDrag, cancelDrag, lostPointerCapture, errorPosition, clearError } = useHandController();
  return <div ref={stage} tabIndex={-1} className={`hand-stage hand-${phase}`} data-side={snapshot.settings.side} data-preview={snapshot.preview || undefined}
    data-hidden={hidden || undefined} data-expanded={snapshot.expanded} data-input-mode={inputMode}
    style={{ "--hand-card-height": `${cardSize.height}px`, width: surface?.width, height: surface?.height,
      transform: surface ? `scale(${surface.scale}) translate(${-surface.left}px, ${-surface.top}px)` : undefined } as CSSProperties} onContextMenu={(e) => e.preventDefault()}>
    {(!desktop || snapshot.preview) && <HandDock element={toolbarElement} point={dockPosition} areas={dockAreas} expanded={snapshot.expanded} preview={snapshot.preview}
      diameter={diameter} count={snapshot.cards.length} capacity={snapshot.settings.capacity} busy={!!busy} confirm={clearConfirm}
      toCanvas={toCanvas} dragging={setDockMoving}
      beginDrag={() => Promise.resolve()}
      move={moveDock} toggle={toggleDock} clear={clearDock} />}
    {error && <div className="hand-error" role="alert" data-hand-hit style={errorPosition} onClick={clearError}>{error}</div>}
    {cardsMounted && visibleCards.map((card) => {
      const removing = discardById.get(card.id);
      const flyingCard = flying?.card.id === card.id ? flying : null;
      const i = cardIndices.get(card.id) ?? -1;
      const pose = removing?.pose ?? flyingCard?.pose ?? poses[i]; const held = !removing && !flyingCard && drag?.id === card.id ? drag : null;
      const playing = !!held && held.playable;
      const style = { "--hand-x": `${held ? held.pose.x : pose.x}px`, "--hand-y": `${held ? held.pose.y : pose.y}px`, width: pose.width, height: pose.height,
        "--hand-base-scale": pose.width / 156,
        zIndex: flyingCard ? 1200 : held ? 1000 : active === card.id ? 500 : i+1,
        "--card-x": `${held ? held.dx : 0}px`, "--card-y": `${held ? held.dy : 0}px`,
        "--hand-angle": `${held ? 0 : pose.angle}deg`,
        "--hand-card-scale": held ? 1.12 : pose.scale, "--hand-order": i, "--discard-delay": `${removing?.delay ?? 0}ms`,
      } as CSSProperties;
      return <div key={card.id} data-hand-id={card.id} data-hand-hit className={`hand-card${active === card.id ? " hand-active" : ""}${held ? " hand-dragging" : ""}${playing ? " hand-playable" : ""}${removing ? " hand-discarding" : ""}${flyingCard ? " hand-flying" : ""}${!removing && !flyingCard && busy === card.id ? " hand-pending" : ""}`}
        role="button" tabIndex={-1} aria-label={card.song.mysteryMode ? t("神秘歌曲") : card.song.name} aria-disabled={snapshot.preview || !!busy}
        style={style} onAnimationEnd={(event) => { if (event.target === event.currentTarget) { removing?.complete(); if (flyingCard) finishFlight(); } }} onPointerDown={(e) => onCardPointerDown(card, e)} onPointerMove={(e) => onCardPointerMove(card, e)} onPointerUp={endDrag} onPointerCancel={cancelDrag} onLostPointerCapture={() => lostPointerCapture(card.id)}>
        <div className="hand-flip"><div className="hand-card-face">
          <HandCardContent card={card} />
          {!snapshot.preview && <button className="hand-discard" aria-label={t("弃掉此牌")} disabled={!!busy}
            onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); void operation("discard_hand_card", card); }}><Icon name="close" size={14} /></button>}
          {playing && <div className="hand-release-label"><Icon name="play" size={10} />{t("松开以播放")}</div>}
        </div>{card.mysteryRevealed && snapshot.arrival?.id === card.id && <div className="hand-card-back"><div className="hand-card-face"><div className="hand-art"><Cover url="" data={snapshot.arrival.mysteryCover} prepared mystery label={t("神秘歌曲")} /></div><strong>???</strong><span>???</span></div></div>}</div>
      </div>;
    })}
  </div>;
}
