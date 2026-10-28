import { type CSSProperties } from "react";
import { desktop } from "../../services/desktop";
import { t } from "../../i18n";
import { Cover } from "../../components/Cover";
import { Icon } from "../../Icon";
import { appLogo } from "../../assets";
import { groupSettings } from "./handLayout";
import { HandDock } from "./HandDock";
import { HandAutoPlayLock } from "./HandAutoPlayLock";
import { HandCardContent } from "./HandCardContent";
import { handSurfaceWaiting } from "./handSurface";
import { useHandController } from "./useHandController";
import { isHandRegionAnimation, isHandRegionTransition } from "./handRegionEvents";
import { useAppearance } from "../settings/useAppearance";
import "../../App.css";
import "./hand.css";

/** Rendering delegates persisted operations, pointer input, motion and native regions to their owners. */
export function HandWindow() {
  const { snapshot, phase, hidden, inputMode, cardsMounted, stage, surface, cardSize, toolbarElement, dockPosition, dockAreas, diameter,
    scheduleRegions, busy, autoLocked, autoNextId, error, clearConfirm, toCanvas, setDockMoving, moveDock, toggleDock, clearDock, visibleCards, discardById,
    flying, drag, poses, cardIndices, active, operation, finishFlight, onCardPointerDown, onCardPointerMove,
    endDrag, cancelDrag, lostPointerCapture, errorPosition, clearError, discoveryPhase, replacingId, refreshDiscovery, restPoses, layoutDiscoveryCount, discoveryCount, ordinaryCount,
    retiringById, entering, finishCardTransition, layoutTransition, arrivalReady } = useHandController();
  useAppearance(snapshot.gui);
  const layoutPending = !!layoutTransition?.pending;
  const liquid = snapshot.discoverySettings?.liquid_strength ?? 100;
  const liquidMode = snapshot.discoverySettings?.liquid_mode ?? "static";
  const glassFilter = liquidMode === "blur" && liquid ? `blur(${.22 * liquid}px) saturate(${100 + .6 * liquid}%)` : "none";
  const boundaryA = restPoses[layoutDiscoveryCount-1], boundaryB = restPoses[layoutDiscoveryCount];
  const vertical = snapshot.settings.side !== "bottom";
  const boundary = snapshot.discoverySettings?.merged && snapshot.settings.enabled && boundaryA && boundaryB ? {
    left: vertical ? boundaryA.x + (snapshot.settings.side === "left" ? boundaryA.width/2+8 : -boundaryA.width/2-32) : (boundaryA.x+boundaryB.x)/2,
    top: vertical ? (boundaryA.y+boundaryB.y)/2 : Math.min(boundaryA.y,boundaryB.y)-boundaryA.height/2-28,
  } : null;
  return <div ref={stage} tabIndex={-1} inert={layoutPending} className={`hand-stage hand-${phase}`} data-side={snapshot.settings.side} data-preview={snapshot.preview || undefined}
    data-hidden={hidden || undefined} data-expanded={snapshot.expanded} data-input-mode={inputMode} data-liquid-mode={liquidMode}
    style={{ "--hand-card-height": `${cardSize.height}px`, "--hand-glass-outline": `${snapshot.discoverySettings?.text_outline_width ?? 1}px`, width: surface?.width, height: surface?.height,
      "--hand-glass-filter": glassFilter,
      "--hand-glass-opacity": `${100 - .54 * liquid}%`, "--hand-glass-highlight": `${.7 * liquid}%`,
      transform: surface ? `scale(${surface.scale}) translate(${-surface.left}px, ${-surface.top}px)` : undefined } as CSSProperties}
    onContextMenu={(e) => e.preventDefault()}
    onTransitionEnd={(event) => { if (isHandRegionTransition(event)) scheduleRegions.current(); }}
    onAnimationEnd={(event) => { if (isHandRegionAnimation(event)) scheduleRegions.current(); }}>
    {(!desktop || snapshot.preview) && <HandDock element={toolbarElement} point={dockPosition} areas={dockAreas} expanded={snapshot.expanded} preview={snapshot.preview}
      diameter={diameter} count={ordinaryCount} capacity={snapshot.settings.capacity} busy={!!busy} confirm={clearConfirm}
      handEnabled={snapshot.settings.enabled || snapshot.preview && !snapshot.discoverySettings?.enabled} createCards={snapshot.settings.create_cards} discovery={snapshot.discovery} refresh={refreshDiscovery}
      toCanvas={toCanvas} dragging={setDockMoving}
      beginDrag={() => Promise.resolve()}
      move={moveDock} toggle={toggleDock} clear={clearDock} />}
    {error && <div className="hand-error" role="alert" data-hand-hit style={errorPosition} onClick={clearError}>{error}</div>}
    {cardsMounted && snapshot.expanded && boundary && <div className="hand-intake-boundary" data-vertical={vertical || undefined} style={boundary} aria-label={t("加入手牌分界")} />}
    {cardsMounted && visibleCards.map((card) => {
      const removing = discardById.get(card.id);
      const retiring = retiringById.get(card.id);
      const flyingCard = flying?.card.id === card.id ? flying : null;
      const i = cardIndices.get(card.id) ?? -1;
      const frozen = layoutPending ? layoutTransition?.poses.get(card.id) : undefined;
      const pose = frozen ?? retiring?.pose ?? removing?.pose ?? flyingCard?.pose ?? poses[i]; const held = !retiring && !removing && !flyingCard && drag?.id === card.id ? drag : null;
      const playing = !!held && held.playable;
      const collecting = !!held && card.discovery && held.intake !== null;
      const settings = retiring ? { side: retiring.side } : groupSettings(card, snapshot.settings, snapshot.discoverySettings);
      const refreshing = !!card.discovery && !flyingCard && (!!discoveryPhase || card.placeholder || replacingId === card.id);
      // Do not paint the next-card ring or the active face highlight while the
      // real arrival/refill/flip surface is still moving. The collected-card
      // arrival is gated by the exact generation, native presentation, cover
      // readiness and arrival/flip completion, never a fixed delay.
      const surfaceWaiting = handSurfaceWaiting(card.id, {
        generation: snapshot.generation,
        arrivalId: snapshot.arrival?.id,
        arrivalReady,
        entering,
        refreshing,
      });
      const autoNext = card.id === autoNextId && !drag && !removing && !retiring && !flyingCard && !layoutPending && !surfaceWaiting;
      const shortcutIndex = snapshot.discoverySettings?.enabled && !snapshot.discoverySettings.merged ? (card.discovery ? -1 : i-discoveryCount) : i;
      const style = { "--hand-x": `${held ? held.pose.x : pose.x}px`, "--hand-y": `${held ? held.pose.y : pose.y}px`, width: pose.width, height: pose.height,
        "--hand-base-scale": pose.width / 156,
        zIndex: flyingCard ? 1200 : held ? 1000 : active === card.id ? 500 : i+1,
        "--card-x": `${held ? held.dx : 0}px`, "--card-y": `${held ? held.dy : 0}px`,
        "--hand-angle": `${held ? 0 : pose.angle}deg`,
        "--hand-card-scale": held ? 1.12 : pose.scale, "--hand-order": i, "--discard-delay": `${removing?.delay ?? 0}ms`,
        "--refill-x": settings.side === "left" ? "-42px" : settings.side === "right" ? "42px" : "0px", "--refill-y": settings.side === "bottom" ? "42px" : "0px",
      } as CSSProperties;
      return <div key={card.id} data-hand-id={card.id} data-auto-locked={autoLocked || undefined} data-side={settings.side} data-hand-hit={retiring ? undefined : true} inert={!!retiring} aria-hidden={!!retiring || undefined} className={`hand-card${card.discovery ? " hand-discovery" : ""}${card.placeholder ? " hand-refill" : ""}${!layoutPending && retiring ? " hand-leaving" : !layoutPending && entering.has(card.id) ? " hand-entering" : ""}${layoutPending && !frozen && !retiring ? " hand-pending" : ""}${refreshing ? " hand-refreshing" : ""}${surfaceWaiting ? " hand-surface-waiting" : ""}${active === card.id ? " hand-active" : ""}${held ? " hand-dragging" : ""}${drag?.presented && drag.id === card.id ? " hand-drag-source" : ""}${playing || collecting ? " hand-playable" : ""}${removing ? " hand-discarding" : ""}${flyingCard ? flyingCard.collecting ? " hand-transferring" : " hand-flying" : ""}${!removing && !flyingCard && busy === card.id && !card.discovery ? " hand-pending" : ""}`}
        role="button" tabIndex={-1} data-auto-next={autoNext || undefined} aria-label={`${card.song.mysteryMode ? t("神秘歌曲") : card.song.name}${autoNext ? ` · ${t("下一张自动打出")}` : ""}`} title={autoNext ? t("下一张自动打出") : undefined} aria-disabled={snapshot.preview || !!busy || autoLocked}
        style={style} onAnimationEnd={(event) => { if (event.target === event.currentTarget) { removing?.complete(); if (flyingCard) finishFlight(); finishCardTransition(card.id); } }} onPointerDown={(e) => onCardPointerDown(card, e)} onPointerMove={(e) => onCardPointerMove(card, e)} onPointerUp={endDrag} onPointerCancel={cancelDrag} onLostPointerCapture={() => lostPointerCapture(card.id)}>
        <div className="hand-flip"><div className="hand-card-face">
          <HandCardContent card={card} />
          {!snapshot.preview && !card.placeholder && (!card.discovery || snapshot.discovery?.replacementEnabled) && <button className="hand-discard" aria-label={card.discovery ? t("替换这张迷你发现卡") : t("弃掉此牌")} disabled={!!busy || !!card.discovery && !snapshot.discovery?.replacementsRemaining}
            onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); void operation("discard_hand_card", card); }}><Icon name={card.discovery ? "refresh" : "close"} size={14} /></button>}
          {!card.placeholder && shortcutIndex >= 0 && shortcutIndex < 9 && <span className="hand-card-shortcut">{snapshot.settings.play_shortcuts[shortcutIndex]}</span>}
          {(playing || collecting) && <div className="hand-release-label"><Icon name={collecting ? "hand" : "play"} size={10} />{t(collecting ? "松开以加入手牌" : "松开以播放")}</div>}
        </div>{card.discovery ? <div className="hand-card-back hand-discovery-back"><div className="hand-card-face"><img src={appLogo} draggable={false} alt="DiscoAS" /></div></div> : card.mysteryRevealed && snapshot.arrival?.id === card.id && <div className="hand-card-back"><div className="hand-card-face"><div className="hand-art"><Cover url="" data={snapshot.arrival.mysteryCover} prepared mystery label={t("神秘歌曲")} /></div><strong>???</strong><span>???</span></div></div>}</div>
        <HandAutoPlayLock locked={autoLocked} />
      </div>;
    })}
  </div>;
}
