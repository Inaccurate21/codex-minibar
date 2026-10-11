import React from "react";
import { continueRender, delayRender, staticFile } from "remotion";
import { GEIST, SEGOE } from "./palette";

/// Holds the frame until every face the scenes measure with canvas is loaded.
/// Render tabs otherwise measure text with different fallbacks, and widths
/// computed from those measurements jump between frames.
export const FontGate: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [handle] = React.useState(() => delayRender("fonts"));
  const [ready, setReady] = React.useState(false);
  React.useEffect(() => {
    const faces = [400, 500, 600, 700].flatMap((weight) => [`${weight} 14px ${SEGOE}`, `${weight} 14px ${GEIST}`]);
    Promise.all(faces.map((face) => document.fonts.load(face)))
      .then(() => document.fonts.ready)
      .then(() => {
        setReady(true);
        continueRender(handle);
      });
  }, [handle]);
  return (
    <>
      <Fonts />
      {ready && children}
    </>
  );
};

export const Fonts: React.FC = () => (
  <style>
    {[400, 500, 600, 700]
      .map(
        (weight) => `@font-face { font-family: "Geist"; font-weight: ${weight}; src: url("${staticFile(
          `fonts/Geist-${{ 400: "Regular", 500: "Medium", 600: "SemiBold", 700: "Bold" }[weight]}.ttf`,
        )}"); }`,
      )
      .join("\n")}
    {/* Unhinted glyphs scale smoothly; hinted ones snap to the pixel grid and
        jitter while a camera zooms. */}
    {"* { text-rendering: geometricPrecision; -webkit-font-smoothing: antialiased; }"}
  </style>
);

/// A Windows 11 style bloom wallpaper drawn in CSS, so the acrylic popup
/// has color to blur. `drift` (0..1) slowly moves the blooms.
export const Desktop: React.FC<{ dark?: boolean; drift?: number; children?: React.ReactNode }> = ({
  dark = true,
  drift = 0,
  children,
}) => {
  const d = drift * 60;
  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        overflow: "hidden",
        background: dark
          ? "linear-gradient(135deg, #0b0f1e 0%, #121226 45%, #1a1020 100%)"
          : "linear-gradient(135deg, #dfe8f7 0%, #eef0f8 50%, #f6e9e4 100%)",
      }}
    >
      {[
        { x: 18, y: 30, s: 900, c: dark ? "#3a2bd6" : "#7fa6ff", o: 0.55 },
        { x: 70, y: 75, s: 1000, c: dark ? "#d2643c" : "#ffb38a", o: 0.45 },
        { x: 85, y: 15, s: 700, c: dark ? "#7a2bb8" : "#d9a6ff", o: 0.4 },
        { x: 35, y: 90, s: 800, c: dark ? "#0f6a8a" : "#9fe3f2", o: 0.4 },
      ].map((b, i) => (
        <div
          key={i}
          style={{
            position: "absolute",
            left: `calc(${b.x}% - ${b.s / 2}px + ${Math.sin(i + d / 30) * d}px)`,
            top: `calc(${b.y}% - ${b.s / 2}px + ${Math.cos(i * 2 + d / 30) * d}px)`,
            width: b.s,
            height: b.s,
            borderRadius: "50%",
            background: b.c,
            opacity: b.o,
            filter: "blur(140px)",
          }}
        />
      ))}
      {children}
    </div>
  );
};
