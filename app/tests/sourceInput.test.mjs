import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs";
import ts from "typescript";

const source = fs.readFileSync(new URL("../src/features/library/sourceInput.ts", import.meta.url), "utf8");
const compiled = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ES2020, module: ts.ModuleKind.ESNext },
}).outputText;
const { identifySource, sourceKinds } = await import(`data:text/javascript;base64,${Buffer.from(compiled).toString("base64")}`);

test("new platform controls expose only the source kinds the backend supports", () => {
  assert.deepEqual(sourceKinds("YouTube"), ["playlist", "video"]);
  assert.deepEqual(sourceKinds("Bilibili"), ["video", "favorites", "collection", "series"]);
  for (const platform of ["NeteaseCloudMusic", "QQMusic", "KugouMusic", "Spotify", "KuwoMusic", "QishuiMusic"])
    assert.deepEqual(sourceKinds(platform), ["playlist", "album"]);
});

test("YouTube watch and short links identify videos and public playlists", () => {
  for (const url of ["https://www.youtube.com/watch?v=dQw4w9WgXc", "https://youtu.be/dQw4w9WgXc?t=30",
    "https://music.youtube.com/watch?v=dQw4w9WgXc", "https://m.youtube.com/watch?v=dQw4w9WgXc"])
    assert.deepEqual(identifySource(url), { platform: "YouTube", kind: "video" });
  for (const url of ["https://www.youtube.com/playlist?list=PLexample", "https://www.youtube.com/watch?v=dQw4w9WgXc&list=PLexample"])
    assert.deepEqual(identifySource(url), { platform: "YouTube", kind: "playlist" });
  assert.deepEqual(identifySource("分享一个视频：https://youtu.be/dQw4w9WgXc。"), { platform: "YouTube", kind: "video" });
  assert.deepEqual(identifySource("推荐歌单 https://www.youtube.com/playlist?list=PLexample\n复制链接观看"), { platform: "YouTube", kind: "playlist" });
});

test("Bilibili BV and numbered parts remain video source inputs", () => {
  for (const value of ["BV1xx411c7mD", "BV1xx411c7mD_p2", "https://www.bilibili.com/video/BV1xx411c7mD?p=2",
    "https://b23.tv/example"])
    assert.deepEqual(identifySource(value), { platform: "Bilibili", kind: "video" });
  assert.deepEqual(identifySource("【分享视频】 https://www.bilibili.com/video/BV1xx411c7mD?p=2，欢迎观看"), { platform: "Bilibili", kind: "video" });
});

test("Bilibili favorites, collections and series select separate import controls", () => {
  for (const url of ["https://space.bilibili.com/1/favlist?fid=123", "https://www.bilibili.com/medialist/detail/ml123",
    "https://www.bilibili.com/medialist/play/1?media_id=123"])
    assert.deepEqual(identifySource(url), { platform: "Bilibili", kind: "favorites" });
  for (const url of ["https://space.bilibili.com/1/lists/123?type=season", "https://space.bilibili.com/1/channel/collectiondetail?sid=123",
    "https://space.bilibili.com/1/channel/detail?season_id=123"])
    assert.deepEqual(identifySource(url), { platform: "Bilibili", kind: "collection" });
  for (const url of ["https://space.bilibili.com/1/lists/123?type=series", "https://space.bilibili.com/1/channel/seriesdetail?sid=123",
    "https://space.bilibili.com/1/channel/detail?series_id=123"])
    assert.deepEqual(identifySource(url), { platform: "Bilibili", kind: "series" });
});

test("existing music platform sharing links keep their album or playlist choice", () => {
  assert.deepEqual(identifySource("https://music.163.com/#/playlist?id=123"), { platform: "NeteaseCloudMusic", kind: "playlist" });
  assert.deepEqual(identifySource("https://music.163.com/#/album?id=123"), { platform: "NeteaseCloudMusic", kind: "album" });
  assert.deepEqual(identifySource("https://y.qq.com/n/ryqq/albumDetail/abc"), { platform: "QQMusic", kind: "album" });
  assert.deepEqual(identifySource("spotify:album:example"), { platform: "Spotify", kind: "album" });
  assert.deepEqual(identifySource("https://open.spotify.com/playlist/example"), { platform: "Spotify", kind: "playlist" });
  assert.deepEqual(identifySource("https://www.kugou.com/songlist/example"), { platform: "KugouMusic", kind: "playlist" });
});

test("source recognition does not mistake unrelated or lookalike hosts for supported sites", () => {
  for (const value of ["", "a random ID", "https://youtube.com.example.invalid/watch?v=123",
    "https://bilibili.com.example.invalid/video/BV1xx411c7mD", "https://example.invalid/?next=https://youtube.com/watch?v=123",
    "https://example.invalid/playlist?list=123"])
    assert.equal(identifySource(value), null);
});

test("Kuwo and Qishui public sources select the matching platform and kind", () => {
  for (const [source, platform, kind] of [
    ["分享 https://www.kuwo.cn/playlist_detail/3677150229", "KuwoMusic", "playlist"],
    ["https://m.kuwo.cn/newh5app/album_detail/435", "KuwoMusic", "album"],
    ["https://www.kuwo.cn/#/album_detail/435", "KuwoMusic", "album"],
    ["https://www.qishui.com/share/playlist?playlist_id=7624447086608777235", "QishuiMusic", "playlist"],
    ["https://music.douyin.com/qishui/share/album?album_id=7692011978622584882", "QishuiMusic", "album"],
  ]) assert.deepEqual(identifySource(source), { platform, kind });
  for (const url of ["https://www.qishui.com.evil.invalid/share/playlist?playlist_id=1", "https://kuwo.cn.evil.invalid/album_detail/1", "https://www.douyin.com/video/123"])
    assert.equal(identifySource(url), null);
});
