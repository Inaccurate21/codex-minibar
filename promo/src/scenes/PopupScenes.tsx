import React from "react";
import { AbsoluteFill, interpolate, useCurrentFrame, useVideoConfig } from "remotion";
import { Desktop, Fonts } from "../ui/Desktop";
import { DEFAULT_TABS, Footer, HomePage, POPUP_WIDTH, PopupFrame } from "../ui/Popup";
import { Icon, ProviderMark, Segmented, T } from "../ui/kit";
import { QuotaGroup, ResetRow } from "../ui/cards";
import { UsagePage } from "../ui/Usage";
import { Palette, dracula, fluent, solarized, vercel } from "../ui/palette";
import { Layout, STRINGS, sections } from "../data";
import { easeOutCubic } from "../ui/Toast";

/// Playback speed of the clip scenes; the launch video runs them faster.
export const Speed = React.createContext(1);
/// Seconds of the enclosing sequence before the scene's own time 0.
export const Offset = React.createContext(0);

const useTime = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const offset = React.useContext(Offset);
  return Math.max(0, frame / fps - offset) * React.useContext(Speed);
};

/// Popup centered on the bloom desktop; `bare` drops the desktop so a
/// composition can be rendered with a transparent background.
const Stage: React.FC<{ dark?: boolean; children: React.ReactNode; bare?: boolean; drift?: number }> = ({
  dark = true,
  children,
  bare,
  drift = 0,
}) => (
  <AbsoluteFill>
    <Fonts />
    {!bare && <Desktop dark={dark} drift={drift} />}
    <AbsoluteFill style={{ alignItems: "center", justifyContent: "center" }}>{children}</AbsoluteFill>
  </AbsoluteFill>
);

/* --------------------------------------------------------------- layouts */

const LAYOUTS: Layout[] = ["cards", "lines", "rings", "tiles"];
export const LAYOUT_STEP = 1.6;
export const LAYOUTS_SECONDS = LAYOUT_STEP * LAYOUTS.length;

const layoutHeight = (layout: Layout, p: Palette) =>
  layout === "cards" ? 58 * 2 + p.cardGap : layout === "lines" ? 44 * 2 + p.cardGap : layout === "rings" ? 72 : 58;

/// Home edit mode: one provider block switching between its four layouts.
export const LayoutSwitcher: React.FC<{ time: number; p?: Palette }> = ({ time, p = fluent(true) }) => {
  const s = STRINGS.en.s;
  const section = sections()[0];
  const step = Math.floor(time / LAYOUT_STEP) % LAYOUTS.length;
  const local = time - Math.floor(time / LAYOUT_STEP) * LAYOUT_STEP;
  const prev = (step + LAYOUTS.length - 1) % LAYOUTS.length;
  const k = time < LAYOUT_STEP ? 1 : easeOutCubic(local / 0.42);
  const height = layoutHeight(LAYOUTS[prev], p) + (layoutHeight(LAYOUTS[step], p) - layoutHeight(LAYOUTS[prev], p)) * k;
  const glide = time < LAYOUT_STEP ? step : prev + (step - prev) * easeOutCubic(local / 0.3);
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: p.cardGap }}>
      <div style={{ display: "flex", alignItems: "center", height: 28, padding: "0 4px" }}>
        <ProviderMark provider="claude" size={16} p={p} badge={section.badge} />
        <div style={{ width: 14 }} />
        <T size={14} color={p.textPrimary} weight={600}>
          Claude
        </T>
        <T size={14} color={p.textTertiary} style={{ marginLeft: 4 }}>
          Max
        </T>
        <div style={{ flex: 1 }} />
        <T size={12} color={p.textTertiary} style={{ marginRight: 8 }}>
          Personal
        </T>
        <Icon name="fluent-re-order-dots-vertical-16-filled" size={14} color={p.textSecondary} />
      </div>
      <div style={{ display: "flex", alignItems: "center", padding: "0 4px" }}>
        <T size={12} color={p.textSecondary}>
          Layout
        </T>
        <div style={{ flex: 1 }} />
        <Segmented items={["Cards", "Lines", "Rings", "Tiles"]} selected={step} glide={glide} p={p} />
      </div>
      <div style={{ position: "relative", height }}>
        {[prev, step].map((index, i) => {
          if (time < LAYOUT_STEP && i === 0) return null;
          const opacity = i === 1 ? k : 1 - k;
          if (opacity <= 0.001) return null;
          return (
            <div
              key={`${i}-${index}`}
              style={{
                position: "absolute",
                inset: "0 0 auto 0",
                opacity,
                transform: `translateY(${(i === 1 ? 1 - k : -k) * 6}px)`,
              }}
            >
              <QuotaGroup quotas={section.quotas} layout={LAYOUTS[index]} p={p} s={s} />
            </div>
          );
        })}
      </div>
      <ResetRow count={1} expires="11d 4h" p={p} s={s} />
    </div>
  );
};

export const Layouts: React.FC<{ bare?: boolean }> = ({ bare }) => {
  const time = useTime();
  const p = fluent(true);
  return (
    <Stage bare={bare} drift={time / 20}>
      <PopupFrame p={p} height={344} footer={<Footer p={p} active={0} editing />}>
        <div style={{ padding: "12px 16px" }}>
          <LayoutSwitcher time={time} p={p} />
        </div>
      </PopupFrame>
    </Stage>
  );
};

/* ---------------------------------------------------------------- themes */

export const THEMES: { label: string; palette: () => Palette }[] = [
  { label: "Fluent · Dark", palette: () => fluent(true) },
  { label: "Fluent · Light", palette: () => fluent(false) },
  { label: "Vercel", palette: () => vercel(true) },
  { label: "VS Code · Dracula", palette: () => dracula() },
  { label: "VS Code · Solarized Light", palette: () => solarized() },
];
export const THEME_STEP = 1.8;
export const THEMES_SECONDS = THEME_STEP * THEMES.length;

/// Index and blend factor of a crossfading cycle.
const cycle = (time: number, step: number, count: number, fade = 0.45) => {
  const index = Math.floor(time / step) % count;
  const local = time - Math.floor(time / step) * step;
  const k = local > step - fade ? easeOutCubic((local - (step - fade)) / fade) : 0;
  return { index, next: (index + 1) % count, k };
};

const HOME_HEIGHT = 836;

export const Themes: React.FC<{ bare?: boolean; showLabel?: boolean }> = ({ bare, showLabel = true }) => {
  const time = useTime();
  const { index, next, k } = cycle(time, THEME_STEP, THEMES.length);
  const layer = (i: number, opacity: number) => {
    const p = THEMES[i].palette();
    return (
      <div key={i} style={{ position: "absolute", inset: 0, opacity, display: "flex", alignItems: "center", justifyContent: "center" }}>
        {!bare && <Desktop dark={p.dark} drift={time / 20} />}
        <PopupFrame p={p} height={HOME_HEIGHT} footer={<Footer p={p} active={0} />}>
          <HomePage p={p} s={STRINGS.en.s} sections={sections()} limit={3} />
        </PopupFrame>
      </div>
    );
  };
  const label = k < 0.5 ? THEMES[index].label : THEMES[next].label;
  return (
    <AbsoluteFill>
      <Fonts />
      {layer(index, 1)}
      {k > 0 && layer(next, k)}
      {showLabel && (
        <div
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            bottom: 18,
            display: "flex",
            justifyContent: "center",
          }}
        >
          <div
            style={{
              padding: "6px 14px",
              borderRadius: 999,
              background: "rgba(0,0,0,0.55)",
              color: "#fff",
              fontFamily: fluent(true).font,
              fontSize: 15,
              fontWeight: 600,
              opacity: 1 - Math.sin(Math.PI * k) * 0.8,
            }}
          >
            {label}
          </div>
        </div>
      )}
    </AbsoluteFill>
  );
};

/* ------------------------------------------------------------- languages */

export const LANGS = ["en", "ru", "de", "fr", "es", "pt-BR", "ja", "zh-CN"];
export const LANG_STEP = 1.1;
export const LANGUAGES_SECONDS = LANG_STEP * LANGS.length;

export const Languages: React.FC<{ bare?: boolean; showLabel?: boolean }> = ({ bare, showLabel = true }) => {
  const time = useTime();
  const { index, next, k } = cycle(time, LANG_STEP, LANGS.length, 0.3);
  const p = fluent(true);
  const page = (lang: string, opacity: number) => (
    <div key={lang} style={{ position: "absolute", inset: 0, opacity }}>
      <HomePage p={p} s={STRINGS[lang].s} sections={sections()} limit={2} />
    </div>
  );
  return (
    <Stage bare={bare} drift={time / 20}>
      <div style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 18 }}>
        <PopupFrame p={p} height={720} footer={<Footer p={p} active={0} />}>
          {page(LANGS[index], 1 - k)}
          {k > 0 && page(LANGS[next], k)}
        </PopupFrame>
        {showLabel && (
          <div style={{ fontFamily: p.font, fontSize: 18, fontWeight: 600, color: "#fff", height: 24 }}>
            {(k < 0.5 ? STRINGS[LANGS[index]] : STRINGS[LANGS[next]]).label}
          </div>
        )}
      </div>
    </Stage>
  );
};

/* ----------------------------------------------------------------- usage */

export const USAGE_SECONDS = 5;

/// `scrollTo` scrolls the page down by that many pixels once the chart is drawn.
export const Usage: React.FC<{ bare?: boolean; scrollTo?: number }> = ({ bare, scrollTo = 0 }) => {
  const time = useTime();
  const p = fluent(true);
  const draw = easeOutCubic(interpolate(time, [0.3, 2.6], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const scroll = scrollTo * easeOutCubic(interpolate(time, [2.5, 3.5], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  return (
    <Stage bare={bare} drift={time / 20}>
      <PopupFrame p={p} height={712} footer={<Footer p={p} active={1} />}>
        <UsagePage p={p} draw={draw} scroll={scroll} />
      </PopupFrame>
    </Stage>
  );
};

/* ------------------------------------------------------------------ home */

export const HOME_SECONDS = 5;

/// Home: the popup rises from the tray, totals count up and the period
/// switches from Today to Yesterday.
export const Home: React.FC<{ bare?: boolean }> = ({ bare }) => {
  const time = useTime();
  const p = fluent(true);
  const open = easeOutCubic(time / 0.25);
  const usage = easeOutCubic(interpolate(time, [0.2, 1.4], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" }));
  const period = interpolate(time, [2.8, 3.1], [0, 1], { extrapolateLeft: "clamp", extrapolateRight: "clamp" });
  return (
    <Stage bare={bare} drift={time / 20}>
      <PopupFrame
        p={p}
        height={1060}
        footer={<Footer p={p} active={0} />}
        style={{ transform: `translateY(${(1 - open) * 40}px)`, opacity: open }}
      >
        <HomePage p={p} s={STRINGS.en.s} sections={sections()} usageProgress={usage} period={easeOutCubic(period)} />
      </PopupFrame>
    </Stage>
  );
};

export const POPUP_STAGE = { width: POPUP_WIDTH + 140, height: 1120 };
