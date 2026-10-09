const paths: Record<string, string> = {
  library: "M4 5h16M4 10h16M4 15h10M4 20h10m5-6v6m-3-3h6",
  discover: "m12 3 2.4 6.6L21 12l-6.6 2.4L12 21l-2.4-6.6L3 12l6.6-2.4L12 3Z",
  // Curved arrows enter the tip diagonally so the shaft cannot overlap an arrowhead arm.
  history: "M5.636 18.364a9 9 0 1 0 0-12.728L3 8.272M3 3.272v5h5M12 7v5l3 2",
  settings:
    "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8Zm8 4 1.3-2.2-2-3.4-2.6.1-1.3-2.3H9.6L8.3 6.5l-2.6-.1-2 3.4L5 12l-1.3 2.2 2 3.4 2.6-.1 1.3 2.3h5.8l1.3-2.3 2.6.1 2-3.4L20 12Z",
  plus: "M12 5v14M5 12h14",
  close: "m6 6 12 12M6 18 18 6",
  arrow: "M5 12h14m-6-6 6 6-6 6",
  chevron: "m6 9 6 6 6-6",
  refresh:
    "M4 12a8 8 0 0 1 13.657-5.657L20 8.686M20 4v4.686h-4.686M20 12a8 8 0 0 1-13.657 5.657L4 15.314M4 20v-4.686h4.686",
  search: "M10.5 4a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13ZM16 16l5 5",
  folder: "M3 7V5h6l2 2h10v12H3V7Z",
  import: "M12 3v12m-4-4 4 4 4-4M4 15v6h16v-6",
  check: "m5 12 4 4L19 6",
  play: "m8 4 12 8-12 8V4Z",
  music:
    "M9 18V5l11-2v13M9 6l11-2M9 18a3 3 0 1 1-3-3 3 3 0 0 1 3 3Zm11-2a3 3 0 1 1-3-3 3 3 0 0 1 3 3Z",
  remove: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 10v7m4-7v7",
  link: "m10 14 4-4m-5 7-2 2a4 4 0 0 1-6-6l5-5a4 4 0 0 1 6 0m0-1 2-2a4 4 0 0 1 6 6l-5 5a4 4 0 0 1-6 0",
  info: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18Zm0 8v6m0-10v.1",
  edit: "m15 4 5 5M4 20l5-1L20 8a3.5 3.5 0 0 0-5-5L4 14v6Z",
};
export function Icon({
  name,
  size = 18,
  className = "",
}: {
  name: string;
  size?: number;
  className?: string;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden="true"
    >
      <path d={paths[name] || paths.music} />
    </svg>
  );
}
