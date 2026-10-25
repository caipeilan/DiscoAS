import { Icon } from "../../Icon";

/** Stays mounted so both locking and unlocking have a visible transition. */
export function HandAutoPlayLock({ locked }: { locked: boolean }) {
  return <div className="hand-auto-lock" data-locked={locked || undefined} aria-hidden="true">
    <Icon name="lock" size={78} />
  </div>;
}
