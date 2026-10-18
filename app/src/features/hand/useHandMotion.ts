import { useEffect, useLayoutEffect } from "react";
import type { HandSnapshot } from "../../types";
import { call, desktop } from "../../services/desktop";
import { errorText } from "../../i18n";
import { arrivalDrawingBounds } from "./handDrawingBounds";
import { animateHandMotion, releaseHandMotion, handRestTransform as restTransform } from "./handMotion";
import type { CardPose } from "./handLayout";
import type { Ref, Setter, Flying, HandMotionRefs } from "./handControllerTypes";
interface MotionContext extends HandMotionRefs {
  snapshot: HandSnapshot; phase: string; hidden: boolean; cardsMounted: boolean; showCardWindow: boolean;
  busy: string; flying: Flying | null; poses: CardPose[]; posesRef: Ref<CardPose[]>; stage: Ref<HTMLDivElement | null>;
  setCardsMounted: Setter<boolean>; setPhase: Setter<string>; setError: Setter<string>;
  commitRegions: Ref<() => Promise<unknown>>; scheduleRegions: Ref<() => void>;
  layoutPrepared?: Ref<(generation: number) => void>;
}
/** The original card-game motion and source-frame handoff remain a single animation responsibility. */
export function useHandMotion(context: MotionContext) {
  const { snapshot, phase, hidden, cardsMounted, showCardWindow, busy, flying, poses, posesRef, stage,
    setCardsMounted, setPhase, setError, commitRegions, scheduleRegions, accepted, unfolding,
    arrivalPlayed, arrivalMotion, arrivalDrawing, returning } = context;
  useLayoutEffect(() => {
    if (phase !== "closed" || !cardsMounted || (!hidden && snapshot.expanded)) return;
    let cancelled = false;
    const frame = requestAnimationFrame(() => {
      const animations = stage.current?.getAnimations({ subtree: true }) ?? [];
      void Promise.all(animations.map((a) => a.finished.catch(() => {}))).then(() => {
        if (!cancelled) setCardsMounted(false);
      });
    });
    return () => { cancelled = true; cancelAnimationFrame(frame); };
  }, [phase, snapshot.expanded, hidden, cardsMounted]);

  useEffect(() => {
    if (!snapshot.generation || hidden) return;
    let cancelled = false, frame = 0;
    const present = (async () => {
      if (!desktop) return;
      if (showCardWindow) await commitRegions.current();
      if (cancelled || snapshot.generation !== accepted.current) return;
      await call("present_hand", { generation: snapshot.generation, showCards: showCardWindow });
    })();
    void present.then(async () => {
      if (cancelled || snapshot.generation !== accepted.current) return;
      context.layoutPrepared?.current(snapshot.generation);
      if (!snapshot.expanded || !showCardWindow) return;
      const arrival = arrivalMotion.current;
      if (arrival?.generation === snapshot.generation) {
        await Promise.all(Array.from(arrival.element.querySelectorAll("img")).map((image) => image.decode().catch(() => {})));
        if (cancelled || snapshot.generation !== accepted.current) return;
        frame = requestAnimationFrame(() => {
          if (cancelled) return;
          unfolding.current = false; setPhase("open");
          // Paint the receiving card at its origin before the discovery window hides.
          frame = requestAnimationFrame(() => {
            if (cancelled) return;
            void (desktop ? call("hand_arrival_ready", { generation: snapshot.generation }) : Promise.resolve()).then(() => {
              if (cancelled || snapshot.generation !== accepted.current) return;
              arrival.animation?.play();
              scheduleRegions.current();
              if (snapshot.cards.find((c) => c.id === snapshot.arrival?.id)?.mysteryRevealed && arrival.animation)
                arrival.element.querySelector(".hand-flip")?.animate([
                  { transform: "rotateY(180deg)" }, { transform: "rotateY(90deg)", offset: .5 }, { transform: "rotateY(0deg)" },
                ], { duration: 420, delay: 120, fill: "backwards" });
            }).catch((e) => setError(errorText(e)));
          });
        });
        return;
      }
      if (!unfolding.current) return;
      // Commit the closed pose before transitioning to the live CSS target.
      frame = requestAnimationFrame(() => { if (!cancelled) { unfolding.current = false; setPhase("open"); } });
    }).catch((e) => {
      if (!cancelled && snapshot.generation === accepted.current) context.layoutPrepared?.current(snapshot.generation);
      setError(errorText(e));
      if (snapshot.arrival && desktop) void call("hand_arrival_ready", { generation: snapshot.generation, error: errorText(e) });
    });
    return () => { cancelled = true; cancelAnimationFrame(frame); };
  }, [snapshot.generation, snapshot.expanded, hidden, showCardWindow]);

  useLayoutEffect(() => {
    const arrival = snapshot.arrival;
    if (hidden || !cardsMounted || !arrival || snapshot.generation <= arrivalPlayed.current) return;
    const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${arrival.id}"]`);
    const pose = posesRef.current[snapshot.cards.findIndex((c) => c.id === arrival.id)];
    if (!element || !pose) return;
    arrivalPlayed.current = snapshot.generation;
    const x = arrival.rect.left + arrival.rect.width / 2;
    const y = arrival.rect.top + arrival.rect.height / 2;
    const animation = window.matchMedia("(prefers-reduced-motion: reduce)").matches ? null : animateHandMotion(element, [
      { transform: `translate(-50%, -50%) translate(${x}px, ${y}px) rotate(0deg) scale(${arrival.rect.width / pose.width})`, opacity: 1 },
      { transform: `translate(-50%, -50%) translate(${x*.45+pose.x*.55}px, ${y*.7+pose.y*.3-80}px) rotate(${pose.angle-8}deg) scale(.8)`, offset: .55 },
      { transform: restTransform, opacity: 1 },
    ], { duration: 480, easing: "cubic-bezier(.22,.75,.24,1)" });
    animation?.pause();
    if (animation) animation.currentTime = 0;
    const motion = { generation: snapshot.generation, element, animation };
    arrivalMotion.current = motion;
    arrivalDrawing.current = animation ? { generation: snapshot.generation, bounds: arrivalDrawingBounds(arrival.rect, pose) } : null;
    const finishArrival = () => {
      if (arrivalMotion.current !== motion) return;
      arrivalDrawing.current = null;
      scheduleRegions.current();
    };
    if (animation) void animation.finished.then(finishArrival, finishArrival);
    return () => {
      releaseHandMotion(element);
      if (arrivalMotion.current === motion) {
        arrivalMotion.current = null;
        arrivalDrawing.current = null;
      }
    };
  }, [snapshot.generation, cardsMounted, hidden]);

  useLayoutEffect(() => {
    if (phase !== "open" || !returning.current.length) return;
    const ready = returning.current.filter((id) => id !== flying?.card.id && id !== busy);
    returning.current = returning.current.filter((id) => !ready.includes(id));
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    ready.forEach((id) => {
      const element = stage.current?.querySelector<HTMLElement>(`[data-hand-id="${id}"]`);
      if (!element) return;
      const pose = poses[snapshot.cards.findIndex((c) => c.id === id)];
      const dx = snapshot.settings.side === "left" ? 150 : snapshot.settings.side === "right" ? -150 : 0;
      const dy = snapshot.settings.side === "bottom" ? -150 : 0;
      animateHandMotion(element, [{ transform: `translate(-50%,-50%) translate(${pose.x+dx}px,${pose.y+dy}px) scale(.85)`, opacity: .4 },
        { transform: restTransform, opacity: 1 }], { duration: 360, easing: "cubic-bezier(.18,.9,.28,1.15)" });
    });
  }, [snapshot, busy, flying, phase]);

}
