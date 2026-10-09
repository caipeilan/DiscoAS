import { platforms } from "./types";

export const platformInfo = (id: string) =>
  platforms.find((platform) => platform.id === id) || platforms[0];
