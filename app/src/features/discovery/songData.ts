import type { Song } from "../../types";

/** Compare DTO fields without serializing potentially large base64 cover strings. */
export function sameSong(next: Song, previous: Song | undefined): boolean {
  if (next === previous) return true;
  if (!previous) return false;
  const keys = Object.keys(next) as Array<keyof Song>;
  if (keys.length !== Object.keys(previous).length) return false;
  return keys.every((key) => {
    const value = next[key];
    const old = previous[key];
    return Array.isArray(value)
      ? Array.isArray(old) && value.length === old.length && value.every((item, index) => item === old[index])
      : value === old;
  });
}
