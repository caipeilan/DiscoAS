import { useCallback, useEffect, useState } from "react";
import { call, onDesktopEvent } from "../../services/desktop";
import { inactiveOnboarding, mergeOnboarding, type OnboardingState } from "./onboardingState";

export function useOnboarding(reportError?: (error: unknown) => void) {
  const [state, setState] = useState(inactiveOnboarding);
  useEffect(() => {
    let active = true, stop: (() => void) | undefined;
    const receive = (next: OnboardingState) => { if (active) setState(current => mergeOnboarding(current, next)); };
    void (async () => {
      stop = await onDesktopEvent("onboarding-changed", receive);
      if (!active) { stop(); return; }
      receive(await call<OnboardingState>("get_onboarding_state"));
    })().catch(error => reportError?.(error));
    return () => { active = false; stop?.(); };
  }, [reportError]);
  const advance = useCallback(async (expected: number, next: number) => {
    const result = await call<OnboardingState>("advance_onboarding", { expected, next });
    setState(current => mergeOnboarding(current, result));
  }, []);
  return { state, advance };
}
