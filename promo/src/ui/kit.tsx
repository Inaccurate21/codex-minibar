import React from "react";
import { staticFile } from "remotion";
import { Palette, brand } from "./palette";

let canvas: HTMLCanvasElement | null = null;
/// Text width in the browser's own layout engine.
export const measureText = (text: string, size: number, weight: number, family: string) => {
  canvas ??= document.createElement("canvas");
  const ctx = canvas.getContext("2d")!;
  ctx.font = `${weight} ${size}px ${family}`;
  return ctx.measureText(text).width;
};

export const PROVIDER_ICON: Record<string, string> = {
  codex: "openai-iconify",
  claude: "claude-iconify",
  cursor: "cursor-iconify",
  openrouter: "openrouter-iconify",
  opencode: "opencode-iconify",
  antigravity: "antigravity",
  grok: "grok",
  kiro: "kiro-iconify",
};

/// A tinted glyph: the SVG is used as an alpha mask, like GPUI's `svg()`
/// with `text_color`.
export const Icon: React.FC<{ name: string; size: number; color: string; style?: React.CSSProperties }> = ({
  name,
  size,
  color,
  style,
}) => {
  const url = `url("${staticFile(`icons/${name}.svg`)}")`;
  return (
    <div
      style={{
        width: size,
        height: size,
        flex: "none",
        backgroundColor: color,
        WebkitMaskImage: url,
        maskImage: url,
        WebkitMaskSize: "contain",
        maskSize: "contain",
        WebkitMaskRepeat: "no-repeat",
        maskRepeat: "no-repeat",
        WebkitMaskPosition: "center",
        maskPosition: "center",
        ...style,
      }}
    />
  );
};

export type Badge = { text: string; color: string };

export const ProviderMark: React.FC<{ provider: string; size: number; p: Palette; badge?: Badge }> = ({
  provider,
  size,
  p,
  badge,
}) => {
  const h = Math.min(14, Math.max(9, size * 0.62));
  return (
    <div style={{ position: "relative", width: size, height: size, flex: "none" }}>
      <Icon name={PROVIDER_ICON[provider]} size={size} color={brand(provider, p)} />
      {badge && (
        <div
          style={{
            position: "absolute",
            right: -h * 0.45,
            bottom: -h * 0.3,
            height: h,
            minWidth: h,
            padding: `0 ${Math.max(2, h * 0.22)}px`,
            borderRadius: h * 0.3,
            background: badge.color,
            border: `1px solid ${p.solidBackground}`,
            boxSizing: "border-box",
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: Math.max(7, h * 0.68),
            lineHeight: `${h}px`,
            fontWeight: 700,
            color: "#fff",
          }}
        >
          {badge.text}
        </div>
      )}
    </div>
  );
};

export const T: React.FC<{
  size?: number;
  line?: number;
  color: string;
  weight?: number;
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ size = 14, line, color, weight = 400, children, style }) => (
  <div
    style={{
      fontSize: size,
      lineHeight: `${line ?? (size === 12 ? 16 : size === 14 ? 20 : size * 1.35)}px`,
      color,
      fontWeight: weight,
      whiteSpace: "nowrap",
      ...style,
    }}
  >
    {children}
  </div>
);

export const cardStyle = (p: Palette): React.CSSProperties => ({
  borderRadius: p.cardRadius,
  background: p.cardBackground,
  border: `1px solid ${p.cardStroke}`,
  boxSizing: "border-box",
  position: "relative",
  overflow: "hidden",
});

/// Segmented control (Today / Yesterday / 30d): accent pill on the selection.
export const Segmented: React.FC<{
  items: string[];
  selected: number;
  p: Palette;
  height?: number;
  /// Fractional selection position while the pill glides.
  glide?: number;
}> = ({ items, selected, p, height = 28, glide }) => {
  const widths = items.map((item) => Math.ceil(measureText(item, 13, 600, p.font)) + 20);
  const at = glide ?? selected;
  const lo = Math.floor(at);
  const hi = Math.min(items.length - 1, lo + 1);
  const t = at - lo;
  const offset = (i: number) => widths.slice(0, i).reduce((a, b) => a + b, 0);
  const left = offset(lo) + (offset(hi) - offset(lo)) * t;
  const width = widths[lo] + (widths[hi] - widths[lo]) * t;
  return (
    <div
      style={{
        position: "relative",
        display: "flex",
        height,
        padding: 2,
        borderRadius: p.controlRadius + 2,
        background: p.controlFill,
        boxSizing: "border-box",
      }}
    >
      <div
        style={{
          position: "absolute",
          top: 2,
          bottom: 2,
          left: 2 + left,
          width,
          borderRadius: p.controlRadius,
          background: p.accent,
        }}
      />
      {items.map((item, i) => (
        <div
          key={item}
          style={{
            position: "relative",
            width: widths[i],
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 13,
            fontWeight: 600,
            color: Math.round(at) === i ? p.textOnAccent : p.textPrimary,
          }}
        >
          {item}
        </div>
      ))}
    </div>
  );
};

export const Button: React.FC<{ label: string; p: Palette; accent?: boolean; tinted?: boolean }> = ({
  label,
  p,
  accent,
  tinted,
}) => (
  <div
    style={{
      height: 32,
      padding: "0 12px",
      display: "flex",
      alignItems: "center",
      borderRadius: p.controlRadius,
      background: accent ? p.accent : tinted ? `color-mix(in srgb, ${p.accent} 22%, transparent)` : p.controlFill,
      border: accent || tinted ? "none" : `1px solid ${p.cardStroke}`,
      color: accent ? p.textOnAccent : tinted ? p.accent : p.textPrimary,
      fontSize: 14,
      fontWeight: 600,
      whiteSpace: "nowrap",
    }}
  >
    {label}
  </div>
);
