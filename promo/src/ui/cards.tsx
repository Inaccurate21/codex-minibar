import React from "react";
import { Palette } from "./palette";
import { Icon, T, cardStyle, Button, measureText } from "./kit";
import { Layout, Pace, Quota, Strings } from "../data";

const RESET_ICON = "fluent-arrow-clockwise-dashes-16-regular";

const titleOf = (q: Quota, s: Strings) =>
  (q.title === "session" ? s.session : q.title === "weekly" ? s.weekly : q.title).toUpperCase();

export const PaceStatus: React.FC<{ pace?: Pace; p: Palette; s: Strings }> = ({ pace, p, s }) => {
  if (!pace) return null;
  if (pace.state === "on") {
    return (
      <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
        <T size={12} color={p.textTertiary}>
          {s.onPace}
        </T>
        <Icon name="fluent-checkmark-20-filled" size={12} color={p.textTertiary} />
      </div>
    );
  }
  const reserve = pace.state === "reserve";
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
      <T size={12} color={p.textTertiary}>
        {pace.delta}%
      </T>
      <Icon
        name={reserve ? "fluent-arrow-down-16-filled" : "fluent-arrow-up-16-filled"}
        size={12}
        color={reserve ? p.positive : p.critical}
      />
    </div>
  );
};

const ResetStatus: React.FC<{ value: string; p: Palette; size?: number }> = ({ value, p, size = 14 }) => (
  <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
    <Icon name={RESET_ICON} size={size === 14 ? 14 : 12} color={p.textSecondary} />
    <T size={size} color={p.textPrimary}>
      {value}
    </T>
  </div>
);

/// Accent fill across the card, interval ticks on the bottom edge and the
/// pace marker on the top edge (compact_progress_layers).
const FillLayers: React.FC<{ q: Quota; p: Palette; fill: number }> = ({ q, p, fill }) => (
  <>
    {fill > 0 && (
      <div
        style={{
          position: "absolute",
          inset: 0,
          clipPath: `inset(0 ${100 - fill}% 0 0)`,
        }}
      >
        <div
          style={{
            position: "absolute",
            inset: 0,
            borderRadius: p.cardRadius - 1,
            background: `color-mix(in srgb, ${p.accent} 20%, transparent)`,
          }}
        />
      </div>
    )}
    {q.ticks > 0 && (
      <div style={{ position: "absolute", inset: 0, display: "flex" }}>
        {Array.from({ length: q.ticks + 1 }, (_, i) => (
          <div key={i} style={{ flex: 1, display: "flex", justifyContent: "flex-end", alignItems: "flex-end" }}>
            {i < q.ticks && (
              <div style={{ width: 2, height: 6, borderRadius: "1px 1px 0 0", background: p.intervalTick }} />
            )}
          </div>
        ))}
      </div>
    )}
    {q.pace && (
      <div
        style={{
          position: "absolute",
          top: 0,
          left: `${q.pace.at}%`,
          width: 2,
          height: 6,
          borderRadius: "0 0 1px 1px",
          background: p.paceMarker,
        }}
      />
    )}
  </>
);

type CardProps = { q: Quota; p: Palette; s: Strings; fill?: number; style?: React.CSSProperties };

/// "Cards": label and value on the left, pace and reset on the right.
export const QuotaCard: React.FC<CardProps> = ({ q, p, s, fill, style }) => {
  const twoRows = !!q.pace;
  return (
    <div style={{ ...cardStyle(p), height: twoRows ? 58 : 44, ...style }}>
      <FillLayers q={q} p={p} fill={fill ?? q.left} />
      <div
        style={{
          position: "relative",
          height: "100%",
          padding: "0 12px",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        {twoRows ? (
          <>
            <div>
              <T size={12} color={p.textSecondary}>
                {titleOf(q, s)}
              </T>
              <T size={14} color={p.accent} weight={600}>
                {s.left(q.left)}
              </T>
            </div>
            <div style={{ display: "flex", flexDirection: "column", alignItems: "flex-end", gap: 2 }}>
              <PaceStatus pace={q.pace} p={p} s={s} />
              <ResetStatus value={q.reset} p={p} />
            </div>
          </>
        ) : (
          <>
            <T size={12} color={p.textSecondary}>
              {titleOf(q, s)}
            </T>
            <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
              <T size={14} color={p.accent} weight={600}>
                {s.left(q.left)}
              </T>
              <ResetStatus value={q.reset} p={p} />
            </div>
          </>
        )}
      </div>
    </div>
  );
};

/// "Lines": one 44 px row per quota.
export const QuotaLine: React.FC<CardProps> = ({ q, p, s, fill, style }) => (
  <div style={{ ...cardStyle(p), height: 44, ...style }}>
    <FillLayers q={q} p={p} fill={fill ?? q.left} />
    <div
      style={{
        position: "relative",
        height: "100%",
        padding: "0 12px",
        display: "flex",
        alignItems: "center",
        gap: 8,
      }}
    >
      <T size={12} color={p.textSecondary}>
        {titleOf(q, s)}
      </T>
      <PaceStatus pace={q.pace} p={p} s={s} />
      <div style={{ flex: 1 }} />
      <T size={14} color={p.accent} weight={600}>
        {s.left(q.left)}
      </T>
      <div style={{ width: 4 }} />
      <ResetStatus value={q.reset} p={p} size={12} />
    </div>
  </div>
);

export const Ring: React.FC<{ value: number; pace?: number; p: Palette; size?: number }> = ({
  value,
  pace,
  p,
  size = 46,
}) => {
  const stroke = 5;
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  const v = Math.max(0, Math.min(100, value));
  return (
    <div style={{ position: "relative", width: size, height: size, flex: "none" }}>
      <svg width={size} height={size} style={{ transform: "rotate(-90deg)" }}>
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={p.accent}
          strokeOpacity={0.2}
          strokeWidth={stroke}
        />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          stroke={p.accent}
          strokeWidth={stroke}
          strokeLinecap="round"
          strokeDasharray={`${(c * v) / 100} ${c}`}
        />
        {pace !== undefined && (
          <line
            x1={size / 2 + (r - stroke / 2 - 1) * Math.cos((pace / 100) * 2 * Math.PI)}
            y1={size / 2 + (r - stroke / 2 - 1) * Math.sin((pace / 100) * 2 * Math.PI)}
            x2={size / 2 + (r + stroke / 2 + 1) * Math.cos((pace / 100) * 2 * Math.PI)}
            y2={size / 2 + (r + stroke / 2 + 1) * Math.sin((pace / 100) * 2 * Math.PI)}
            stroke={p.paceMarker}
            strokeWidth={2}
          />
        )}
      </svg>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          fontSize: 12,
          fontWeight: 600,
          color: p.textPrimary,
        }}
      >
        {Math.round(v)}%
      </div>
    </div>
  );
};

/// "Rings": a tile with a ring, the label, the reset and the pace.
export const QuotaRing: React.FC<CardProps> = ({ q, p, s, fill, style }) => (
  <div
    style={{
      ...cardStyle(p),
      height: 72,
      padding: "0 12px",
      display: "flex",
      alignItems: "center",
      gap: 12,
      ...style,
    }}
  >
    <Ring value={fill ?? q.left} pace={q.pace?.at} p={p} />
    <div style={{ minWidth: 0 }}>
      <T size={12} color={p.textSecondary}>
        {titleOf(q, s)}
      </T>
      {q.notStarted ? (
        <T size={12} color={p.textTertiary}>
          {s.notStarted}
        </T>
      ) : q.pace?.state === "on" ? (
        <>
          <T size={14} color={p.textPrimary} weight={600}>
            {q.reset}
          </T>
          <PaceStatus pace={q.pace} p={p} s={s} />
        </>
      ) : (
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <T size={14} color={p.textPrimary} weight={600}>
            {q.reset}
          </T>
          <PaceStatus pace={q.pace} p={p} s={s} />
        </div>
      )}
    </div>
  </div>
);

/// "Tiles": two compact tiles side by side.
export const QuotaTile: React.FC<CardProps> = ({ q, p, s, fill, style }) => (
  <div style={{ ...cardStyle(p), height: 58, ...style }}>
    <FillLayers q={{ ...q, ticks: 0 }} p={p} fill={fill ?? q.left} />
    <div
      style={{
        position: "relative",
        height: "100%",
        padding: "0 12px",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between" }}>
        <T size={12} color={p.textSecondary}>
          {titleOf(q, s)}
        </T>
        <PaceStatus pace={q.pace?.state === "on" ? q.pace : undefined} p={p} s={s} />
      </div>
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <T size={14} color={p.accent} weight={600}>
          {q.left}%
        </T>
        <ResetStatus value={q.reset} p={p} size={12} />
      </div>
    </div>
  </div>
);

export const QuotaGroup: React.FC<{
  quotas: Quota[];
  layout: Layout;
  p: Palette;
  s: Strings;
  fills?: Record<string, number>;
}> = ({ quotas, layout, p, s, fills = {} }) => {
  if (layout === "rings" || layout === "tiles") {
    const C = layout === "rings" ? QuotaRing : QuotaTile;
    return (
      <div style={{ display: "flex", gap: p.cardGap }}>
        {quotas.map((q) => (
          <C key={q.id} q={q} p={p} s={s} fill={fills[q.id]} style={{ flex: 1, minWidth: 0 }} />
        ))}
      </div>
    );
  }
  const C = layout === "cards" ? QuotaCard : QuotaLine;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: p.cardGap }}>
      {quotas.map((q) => (
        <C key={q.id} q={q} p={p} s={s} fill={fills[q.id]} />
      ))}
    </div>
  );
};

/// Width left for the reset row inside a card (popup content minus padding).
const RESET_ROW_WIDTH = 380 - 32 - 24;

export const ResetRow: React.FC<{ count: number; expires: string; p: Palette; s: Strings }> = ({
  count,
  expires,
  p,
  s,
}) => {
  // Like the app: shorten to "Use" when the full label does not fit beside the title.
  const title = measureText(s.resets(count), 14, 600, p.font) + 12 + 18 + measureText(expires, 14, 400, p.font) + 12;
  const label = title + measureText(s.useReset, 14, 600, p.font) + 24 <= RESET_ROW_WIDTH ? s.useReset : s.useResetShort;
  return (
  <div
    style={{
      ...cardStyle(p),
      height: 58,
      padding: "0 12px",
      display: "flex",
      alignItems: "center",
      gap: 12,
    }}
  >
    <T size={14} color={p.accent} weight={600}>
      {s.resets(count)}
    </T>
    <div style={{ display: "flex", alignItems: "center", gap: 4 }}>
      <Icon name="fluent-clock-16-regular" size={14} color={p.textSecondary} />
      <T size={14} color={p.textPrimary}>
        {expires}
      </T>
    </div>
    <div style={{ flex: 1 }} />
    <Button label={label} p={p} tinted />
  </div>
  );
};
