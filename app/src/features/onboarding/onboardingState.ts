export interface OnboardingState {
  step: number;
  revision: number;
  completed: boolean;
  targetPlatform: string;
  targetId: string;
  targetKind: string;
}
export const inactiveOnboarding: OnboardingState = { step: 0, revision: 0, completed: true, targetPlatform: "", targetId: "", targetKind: "" };
export function mergeOnboarding(current: OnboardingState, incoming: OnboardingState) {
  return incoming.revision < current.revision ? current : incoming;
}
