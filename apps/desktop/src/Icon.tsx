// SPDX-License-Identifier: GPL-3.0-or-later
// Outline paths from Tabler Icons (MIT, https://tabler.io/icons).

export type IconName =
  | "settings"
  | "book"
  | "newspaper"
  | "chef"
  | "puzzle"
  | "devices"
  | "plus"
  | "dots"
  | "chevron-down"
  | "chevron-right"
  | "x"
  | "arrow-left"
  | "arrow-up"
  | "arrow-down"
  | "check"
  | "link"
  | "download"
  | "refresh";

const PATHS: Record<IconName, string[]> = {
  settings: [
    "M10.325 4.317c.426 -1.756 2.924 -1.756 3.35 0a1.724 1.724 0 0 0 2.573 1.066c1.543 -.94 3.31 .826 2.37 2.37a1.724 1.724 0 0 0 1.065 2.572c1.756 .426 1.756 2.924 0 3.35a1.724 1.724 0 0 0 -1.066 2.573c.94 1.543 -.826 3.31 -2.37 2.37a1.724 1.724 0 0 0 -2.572 1.065c-.426 1.756 -2.924 1.756 -3.35 0a1.724 1.724 0 0 0 -2.573 -1.066c-1.543 .94 -3.31 -.826 -2.37 -2.37a1.724 1.724 0 0 0 -1.065 -2.572c-1.756 -.426 -1.756 -2.924 0 -3.35a1.724 1.724 0 0 0 1.066 -2.573c-.94 -1.543 .826 -3.31 2.37 -2.37c1 .608 2.296 .07 2.572 -1.065z",
    "M9 12a3 3 0 1 0 6 0a3 3 0 0 0 -6 0",
  ],
  book: ["M4 19.5A2.5 2.5 0 0 1 6.5 17H20", "M6.5 2H20v20H6.5A2.5 2.5 0 0 1 4 19.5v-15A2.5 2.5 0 0 1 6.5 2z"],
  newspaper: ["M4 4h16v16H4z", "M8 8h8", "M8 12h4", "M8 16h3", "M14 12h2v4h-2z"],
  chef: ["M6 10a4 4 0 0 1 1-7.87A5 5 0 0 1 17 4a3 3 0 0 1 1 5.83V13H6z", "M6 17h12", "M7 21h10"],
  puzzle: ["M8 3h4v3a2 2 0 1 0 4 0V3h5v5h-3a2 2 0 1 0 0 4h3v5h-5v3a2 2 0 1 1-4 0v-3H3v-5h3a2 2 0 1 0 0-4H3V3z"],
  devices: ["M4 5h12v9H4z", "M8 18h4", "M10 14v4", "M18 8h3v11h-5v-3", "M18.5 16.5h0"],
  plus: ["M12 5l0 14", "M5 12l14 0"],
  dots: ["M4 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0", "M11 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0", "M18 12a1 1 0 1 0 2 0a1 1 0 1 0 -2 0"],
  "chevron-down": ["M6 9l6 6l6 -6"],
  "chevron-right": ["M9 6l6 6l-6 6"],
  x: ["M18 6l-12 12", "M6 6l12 12"],
  "arrow-left": ["M5 12l14 0", "M5 12l6 6", "M5 12l6 -6"],
  "arrow-up": ["M12 5l0 14", "M18 11l-6 -6", "M6 11l6 -6"],
  "arrow-down": ["M12 5l0 14", "M18 13l-6 6", "M6 13l6 6"],
  check: ["M5 12l5 5l10 -10"],
  link: ["M9 15l6 -6", "M11 6l.463 -.536a5 5 0 0 1 7.071 7.072l-.534 .464", "M13 18l-.397 .534a5.068 5.068 0 0 1 -7.127 0a4.972 4.972 0 0 1 0 -7.071l.524 -.463"],
  refresh: ["M20 11a8.1 8.1 0 0 0 -15.5 -2m-.5 -4v4h4", "M4 13a8.1 8.1 0 0 0 15.5 2m.5 4v-4h-4"],
  download: ["M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2 -2v-2", "M7 11l5 5l5 -5", "M12 4l0 12"],
};

/** A small outline icon that takes the text colour; decorative, so the button carries the label. */
export default function Icon({ name, size = 16 }: { name: IconName; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.75} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      {PATHS[name].map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
