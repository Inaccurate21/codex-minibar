import React from "react";
import { Palette } from "./palette";
import { Icon, ProviderMark, Segmented, T, cardStyle } from "./kit";
import { QuotaGroup, ResetRow } from "./cards";
import { SPEND, Section, Strings } from "../data";

export const POPUP_WIDTH = 380;
const PAGE_PADDING = 16;

/// Popup capsule: tinted acrylic over whatever sits behind it, the window
/// hairline and the footer band.
export const PopupFrame: React.FC<{
  p: Palette;
  height: number;
  footer: React.ReactNode;
  children: React.ReactNode;
  style?: React.CSSProperties;
}> = ({ p, height, footer, children, style }) => (
  <div
    style={{
      width: POPUP_WIDTH,
      height,
      borderRadius: 8,
      overflow: "hidden",
      position: "relative",
      fontFamily: p.font,
      color: p.textPrimary,
      background: `color-mix(in srgb, ${p.solidBackground} ${p.capsuleOpacity * 100}%, transparent)`,
      backdropFilter: "blur(60px) saturate(1.4)",
      boxShadow: `0 0 0 1px ${p.windowOutline}, 0 16px 48px rgba(0,0,0,${p.dark ? 0.45 : 0.22})`,
      display: "flex",
      flexDirection: "column",
      ...style,
    }}
  >
    <div style={{ flex: 1, overflow: "hidden", position: "relative" }}>{children}</div>
    {footer}
  </div>
);

export type Tab = { key: string; icon?: string; provider?: string; badge?: { text: string; color: string } };

export const DEFAULT_TABS: Tab[] = [
  { key: "home", icon: "fluent-home-24-filled" },
  { key: "usage", icon: "fluent-data-histogram-24-filled" },
  { key: "codex", provider: "codex" },
  { key: "claude", provider: "claude" },
  { key: "cursor", provider: "cursor" },
  { key: "openrouter", provider: "openrouter" },
  { key: "edit", icon: "fluent-edit-20-filled" },
  { key: "refresh", icon: "fluent-arrow-sync-20-filled" },
  { key: "settings", icon: "fluent-settings-20-filled" },
];

/// Footer action buttons; they sit on the trailing edge, the tabs on the leading edge.
const ACTIONS = new Set(["edit", "refresh", "settings"]);
/// footer.rs (comfortable): 36 px buttons, 2 px apart, 14 / 18 px side padding.
const BUTTON = 36;
const SPACING = 2;
const PAD_LEFT = 14;
const PAD_RIGHT = 18;

/// Footer tab strip; `active` indexes the leading tabs and may be fractional
/// while the indicator glides.
export const Footer: React.FC<{ p: Palette; tabs?: Tab[]; active: number; spin?: number; editing?: boolean }> = ({
  p,
  tabs = DEFAULT_TABS,
  active,
  spin = 0,
  editing,
}) => {
  const leading = tabs.filter((tab) => !ACTIONS.has(tab.key));
  const actions = tabs.filter((tab) => ACTIONS.has(tab.key));
  const lo = Math.floor(active);
  const t = active - lo;
  // Stretch toward the next tab, then contract (the popup's tab indicator).
  const stretch = Math.sin(Math.PI * t) * BUTTON * 0.6;
  const width = 16 + stretch;
  const center = PAD_LEFT + (BUTTON + SPACING) * active + BUTTON / 2;
  const button = (tab: Tab, selected: boolean) => (
    <div key={tab.key} style={{ width: BUTTON, height: BUTTON, flex: "none", display: "flex", alignItems: "center", justifyContent: "center" }}>
      {tab.provider ? (
        <ProviderMark provider={tab.provider} size={18} p={p} badge={tab.badge} />
      ) : (
        <Icon
          name={editing && tab.key === "edit" ? "fluent-checkmark-20-filled" : tab.icon!}
          size={18}
          color={selected ? p.textPrimary : p.chromeIcon}
          style={tab.key === "refresh" ? { transform: `rotate(${spin}deg)` } : undefined}
        />
      )}
    </div>
  );
  return (
    <div
      style={{
        height: 56,
        flex: "none",
        background: p.footerBackground,
        borderTop: `1px solid ${p.divider}`,
        display: "flex",
        alignItems: "center",
        gap: SPACING,
        padding: `0 ${PAD_RIGHT}px 0 ${PAD_LEFT}px`,
        position: "relative",
      }}
    >
      {leading.map((tab, i) => button(tab, Math.round(active) === i))}
      <div style={{ flex: 1 }} />
      {actions.map((tab) => button(tab, !!editing && tab.key === "edit"))}
      <div
        style={{
          position: "absolute",
          bottom: 8,
          height: 3,
          borderRadius: 2,
          left: center - width / 2,
          width,
          background: p.accent,
        }}
      />
    </div>
  );
};

const Heading: React.FC<{ section: Section; p: Palette }> = ({ section, p }) => (
  <div style={{ display: "flex", alignItems: "center", height: 28, marginTop: 8, padding: "0 4px" }}>
    <ProviderMark provider={section.provider} size={16} p={p} badge={section.badge} />
    <div style={{ width: section.badge ? 14 : 8 }} />
    <T size={14} color={p.textPrimary} weight={600}>
      {section.name}
    </T>
    {section.plan && (
      <T size={14} color={p.textTertiary} style={{ marginLeft: 4 }}>
        {section.plan}
      </T>
    )}
    <div style={{ flex: 1 }} />
    {section.account && (
      <T size={12} color={p.textTertiary}>
        {section.account}
      </T>
    )}
  </div>
);

export const Donut: React.FC<{ p: Palette; size?: number; progress?: number; total: string }> = ({
  p,
  size = 104,
  progress = 1,
  total,
}) => {
  const stroke = 18;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const sum = SPEND.reduce((a, b) => a + b.amount, 0);
  let offset = 0;
  const gap = 2;
  return (
    <div style={{ position: "relative", width: size, height: size, flex: "none" }}>
      <svg width={size} height={size} style={{ transform: "rotate(-90deg)" }}>
        {SPEND.map((item) => {
          const len = (item.amount / sum) * c * progress;
          const el = (
            <circle
              key={item.name}
              cx={size / 2}
              cy={size / 2}
              r={r}
              fill="none"
              stroke={item.color}
              strokeWidth={stroke}
              strokeDasharray={`${Math.max(0, len - gap)} ${c}`}
              strokeDashoffset={-offset}
            />
          );
          offset += len;
          return el;
        })}
      </svg>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 18,
          fontWeight: 600,
        }}
      >
        {total}
      </div>
    </div>
  );
};

export const UsageWidget: React.FC<{ p: Palette; s: Strings; progress?: number; period?: number }> = ({
  p,
  s,
  progress = 1,
  period = 1,
}) => {
  const sum = SPEND.reduce((a, b) => a + b.amount, 0) * progress;
  return (
    <div>
      <div style={{ display: "flex", alignItems: "center", height: 40, padding: "0 4px" }}>
        <Icon name="fluent-data-histogram-24-filled" size={16} color={p.textSecondary} />
        <T size={14} color={p.textPrimary} weight={600} style={{ marginLeft: 8 }}>
          {s.usage}
        </T>
        <div style={{ flex: 1 }} />
        <Segmented items={[s.today, s.yesterday, "30d"]} selected={Math.round(period)} glide={period} p={p} />
      </div>
      <div style={{ ...cardStyle(p), padding: 14, display: "flex", alignItems: "center", gap: 16 }}>
        <Donut p={p} progress={progress} total={`$${sum.toFixed(2)}`} />
        <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 6 }}>
          {SPEND.map((item) => (
            <div key={item.name} style={{ display: "flex", alignItems: "center", gap: 8, height: 26 }}>
              <div style={{ width: item.badge ? 22 : 18, flex: "none" }}>
                <ProviderMark provider={item.provider} size={16} p={p} badge={item.badge} />
              </div>
              <T size={14} color={p.textPrimary} weight={600} style={{ flex: 1, overflow: "hidden" }}>
                {item.name}
              </T>
              <T size={14} color={p.textPrimary} weight={600}>
                ${(item.amount * progress).toFixed(2)}
              </T>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
};

export const HomePage: React.FC<{
  p: Palette;
  s: Strings;
  sections: Section[];
  fills?: Record<string, number>;
  usageProgress?: number;
  period?: number;
  /// Number of sections to draw (others are cut off by the frame anyway).
  limit?: number;
  scroll?: number;
}> = ({ p, s, sections, fills, usageProgress, period, limit = 4, scroll = 0 }) => (
  <div
    style={{
      padding: `4px ${PAGE_PADDING}px 16px`,
      display: "flex",
      flexDirection: "column",
      gap: p.cardGap,
      transform: `translateY(${-scroll}px)`,
    }}
  >
    <UsageWidget p={p} s={s} progress={usageProgress} period={period} />
    {sections.slice(0, limit).map((section, i) => (
      <React.Fragment key={i}>
        <Heading section={section} p={p} />
        <QuotaGroup quotas={section.quotas} layout={section.layout} p={p} s={s} fills={fills} />
        {section.resets && <ResetRow count={section.resets.count} expires={section.resets.expires} p={p} s={s} />}
      </React.Fragment>
    ))}
  </div>
);
