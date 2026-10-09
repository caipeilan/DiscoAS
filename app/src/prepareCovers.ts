import type { Song } from "./types";
const decodeImage = async (src: string) => {
  const image = new Image();
  image.src = src;
  await image.decode();
};
export async function prepareCovers(
  songs: Song[],
  fallback: string,
  decode = decodeImage,
): Promise<Song[]> {
  const requests = new Map<string, Promise<boolean>>();
  const prepare = (src: string) => {
    if (!requests.has(src))
      requests.set(
        src,
        decode(src).then(
          () => true,
          () => false,
        ),
      );
    return requests.get(src)!;
  };
  return Promise.all(
    songs.map(async (song) => {
      const src = song.coverDataUri || (song.mysteryMode ? fallback : "");
      if (!src || (await prepare(src))) return song;
      if (song.mysteryMode) await prepare(fallback);
      return {
        ...song,
        coverDataUri: null,
        coverError: "封面加载失败，请稍后重试",
      };
    }),
  );
}
