//! Framework-free view model of the taskbar widgets.
//!
//! Resolves configured indicators against the live limits once, so the
//! renderer, the hover card and the Settings previews all read the same
//! labels, values and colors.

use chrono::{DateTime, Local, Utc};

use crate::{
    instances::ProviderId,
    limits::ProviderLimits,
    settings::{
        LimitValue, TaskbarWidget, TaskbarWidgetStyle, TimeFormat, TrayColorMode, TrayIndicator,
    },
};

pub(crate) type Rgb = [u8; 3];

/// Everything outside the widget list that changes how widgets look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Appearance {
    /// The taskbar (not the app) uses the light theme.
    pub(crate) light: bool,
    pub(crate) colored_icons: bool,
    pub(crate) accent: Rgb,
}

impl Appearance {
    pub(crate) fn text(self) -> Rgb {
        if self.light { [0, 0, 0] } else { [255, 255, 255] }
    }

    /// Fluent `TextFillColorPrimary` opacity on this taskbar.
    pub(crate) fn text_alpha(self) -> f32 {
        if self.light { 0.896 } else { 1.0 }
    }

    /// Fluent `TextFillColorSecondary` opacity on this taskbar.
    pub(crate) fn secondary_alpha(self) -> f32 {
        if self.light { 0.606 } else { 0.786 }
    }

    /// Fluent system status colors, tuned per theme for contrast.
    fn status(self, remaining: Option<u8>) -> Rgb {
        match (remaining, self.light) {
            (Some(0..=15), false) => [255, 153, 164],
            (Some(0..=15), true) => [196, 43, 28],
            (Some(16..=50), false) => [252, 225, 0],
            (Some(16..=50), true) => [157, 93, 0],
            (Some(_), false) => [108, 203, 95],
            (Some(_), true) => [15, 123, 15],
            (None, _) => self.text(),
        }
    }

    fn provider_tint(self, provider: ProviderId) -> Rgb {
        let (red, green, blue) = if self.light {
            crate::provider_registry::light_surface_brand_rgb(provider.kind())
        } else {
            crate::provider_registry::dark_surface_brand_rgb(provider.kind())
        };
        [red, green, blue]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Icon {
    pub(crate) name: &'static str,
    pub(crate) tint: Rgb,
}

/// One resolved indicator.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub(crate) provider: ProviderId,
    pub(crate) label: String,
    /// `42%`, or an em dash while no data has arrived.
    pub(crate) value: String,
    /// Fill fraction of the displayed value, 0..=1.
    pub(crate) progress: Option<f32>,
    /// Bar, ring and value tint.
    pub(crate) color: Rgb,
    /// Whether `color` is a real tint rather than the text color.
    pub(crate) tinted: bool,
    pub(crate) resets_at: Option<DateTime<Utc>>,
    pub(crate) icon: Icon,
    /// Remaining or used, for the hover card's wording.
    pub(crate) limit_value: LimitValue,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WidgetView {
    pub(crate) id: String,
    pub(crate) style: TaskbarWidgetStyle,
    pub(crate) icon: Option<Icon>,
    pub(crate) provider: Option<ProviderId>,
    pub(crate) rows: Vec<Row>,
}

impl WidgetView {
    /// Secondary line of a single-indicator Glance: the label and, when
    /// known, the time left until the window resets.
    pub(crate) fn glance_caption(&self, now: DateTime<Utc>) -> String {
        let Some(row) = self.rows.first() else {
            return String::new();
        };
        match row.resets_at.filter(|reset| *reset > now) {
            Some(reset) => format!("{} \u{00b7} {}", row.label, countdown(reset, now)),
            None => row.label.clone(),
        }
    }
}

/// Compact countdown: `2h 13m`, `3d 4h`, `45m`.
pub(crate) fn countdown(reset: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (reset - now).num_minutes().max(0);
    let (days, hours, minutes) = (minutes / 1_440, (minutes % 1_440) / 60, minutes % 60);
    if days > 0 {
        if hours > 0 {
            crate::i18n::format(
                "days-d-hours-h",
                &[("days", days.to_string()), ("hours", hours.to_string())],
            )
        } else {
            crate::i18n::format("days-d", &[("days", days.to_string())])
        }
    } else if hours > 0 {
        if minutes > 0 {
            crate::i18n::format(
                "hours-h-minutes-m",
                &[
                    ("hours", hours.to_string()),
                    ("minutes", minutes.to_string()),
                ],
            )
        } else {
            crate::i18n::format("hours-h", &[("hours", hours.to_string())])
        }
    } else {
        crate::i18n::format("minutes-m", &[("minutes", minutes.to_string())])
    }
}

/// Local wall-clock time of a reset, with the date once it is not today.
pub(crate) fn reset_clock(reset: DateTime<Utc>) -> String {
    let local = reset.with_timezone(&Local);
    let time = TimeFormat::current().format_hm(local);
    if local.date_naive() == Local::now().date_naive() {
        time
    } else {
        format!("{} {time}", local.format("%a"))
    }
}

fn row_color(indicator: &TrayIndicator, provider: ProviderId, remaining: Option<u8>, appearance: Appearance) -> (Rgb, bool) {
    match indicator.color_mode {
        TrayColorMode::Status => (appearance.status(remaining), remaining.is_some()),
        TrayColorMode::Fixed => (
            [
                indicator.fixed_color.red,
                indicator.fixed_color.green,
                indicator.fixed_color.blue,
            ],
            true,
        ),
        TrayColorMode::Provider => (appearance.provider_tint(provider), true),
        TrayColorMode::Accent => (appearance.accent, true),
        TrayColorMode::Monochrome => (appearance.text(), false),
    }
}

fn icon_for(provider: ProviderId, appearance: Appearance) -> Icon {
    let descriptor = crate::provider_registry::descriptor(provider.kind());
    Icon {
        name: descriptor.icon,
        tint: if appearance.colored_icons {
            appearance.provider_tint(provider)
        } else {
            appearance.text()
        },
    }
}

fn resolve_row(
    indicator: &TrayIndicator,
    limits: &ProviderLimits,
    appearance: Appearance,
) -> Option<Row> {
    let provider = indicator.provider()?;
    let metric =
        crate::widget_data::resolve_metric(provider, limits.get(provider), &indicator.metric_id);
    let label = metric
        .as_ref()
        .map(|metric| metric.label.clone())
        .or_else(|| {
            crate::provider_registry::metric(provider.kind(), &indicator.metric_id)
                .map(|metric| metric.localized_label().to_owned())
        })
        .unwrap_or_else(|| indicator.metric_id.clone());
    let window = metric.map(|metric| metric.window).unwrap_or_default();
    let remaining = window.remaining_percent();
    let displayed = match indicator.limit_value {
        LimitValue::Remaining => remaining,
        LimitValue::Used => window.used_percent.map(|used| used.min(100)),
    };
    let (color, tinted) = row_color(indicator, provider, remaining, appearance);
    Some(Row {
        provider,
        label,
        value: displayed.map_or_else(|| "\u{2014}".to_owned(), |value| format!("{value}%")),
        progress: displayed.map(|value| f32::from(value) / 100.0),
        color,
        tinted,
        resets_at: window.resets_at,
        icon: icon_for(provider, appearance),
        limit_value: indicator.limit_value,
    })
}

pub(crate) fn resolve(
    widget: &TaskbarWidget,
    limits: &ProviderLimits,
    appearance: Appearance,
) -> WidgetView {
    let rows = widget
        .indicators
        .iter()
        .take(widget.style.max_indicators())
        .filter_map(|indicator| resolve_row(indicator, limits, appearance))
        .collect::<Vec<_>>();
    let provider = widget.provider();
    let icon = widget
        .show_icon
        .then(|| provider.map(|provider| icon_for(provider, appearance)))
        .flatten();
    WidgetView {
        id: widget.id.clone(),
        style: widget.style,
        icon,
        provider,
        rows,
    }
}

/// Widgets that have at least one enabled instance, resolved in order.
pub(crate) fn resolve_visible(
    widgets: &[TaskbarWidget],
    enabled: &[ProviderId],
    limits: &ProviderLimits,
    appearance: Appearance,
) -> Vec<WidgetView> {
    widgets
        .iter()
        .filter(|widget| {
            widget
                .indicators
                .iter()
                .filter_map(TrayIndicator::provider)
                .any(|provider| enabled.contains(&provider))
        })
        .map(|widget| resolve(widget, limits, appearance))
        .filter(|view| !view.rows.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        limits::{LimitWindow, RateLimits},
        settings::ProviderKind,
    };

    fn appearance(light: bool) -> Appearance {
        Appearance {
            light,
            colored_icons: true,
            accent: [0, 120, 212],
        }
    }

    fn limits(session_used: u8, weekly_used: u8) -> ProviderLimits {
        let codex = ProviderId::primary(ProviderKind::Codex);
        ProviderLimits::from_entries([(
            codex,
            RateLimits {
                primary: LimitWindow {
                    used_percent: Some(session_used),
                    duration_minutes: Some(300),
                    resets_at: Some(Utc::now() + chrono::Duration::minutes(133)),
                },
                secondary: LimitWindow {
                    used_percent: Some(weekly_used),
                    duration_minutes: Some(10_080),
                    resets_at: Some(Utc::now() + chrono::Duration::days(3)),
                },
                ..RateLimits::default()
            },
        )])
    }

    #[test]
    fn preset_resolves_remaining_percentages_with_status_colors() {
        let widget = TaskbarWidget::for_provider(ProviderId::primary(ProviderKind::Codex));
        let view = resolve(&widget, &limits(58, 95), appearance(false));
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0].value, "42%");
        assert_eq!(view.rows[0].progress, Some(0.42));
        assert_eq!(view.rows[0].color, [252, 225, 0]);
        assert_eq!(view.rows[1].value, "5%");
        assert_eq!(view.rows[1].color, [255, 153, 164]);
        assert!(view.icon.is_some());
    }

    #[test]
    fn used_values_and_light_theme_colors() {
        let mut widget = TaskbarWidget::for_provider(ProviderId::primary(ProviderKind::Codex));
        widget.indicators[0].limit_value = LimitValue::Used;
        let view = resolve(&widget, &limits(10, 0), appearance(true));
        assert_eq!(view.rows[0].value, "10%");
        // Status severity still follows what is left.
        assert_eq!(view.rows[0].color, [15, 123, 15]);
    }

    #[test]
    fn missing_data_shows_a_dash_in_the_text_color() {
        let widget = TaskbarWidget::for_provider(ProviderId::primary(ProviderKind::Codex));
        let view = resolve(&widget, &ProviderLimits::default(), appearance(false));
        assert_eq!(view.rows[0].value, "\u{2014}");
        assert_eq!(view.rows[0].progress, None);
        assert!(!view.rows[0].tinted);
    }

    #[test]
    fn style_caps_the_number_of_rows() {
        let mut widget = TaskbarWidget::for_provider(ProviderId::primary(ProviderKind::Codex));
        let provider = ProviderId::primary(ProviderKind::Codex);
        widget.indicators = vec![
            TrayIndicator::new(provider, "codex.session"),
            TrayIndicator::new(provider, "codex.weekly"),
            TrayIndicator::new(provider, "codex.session"),
        ];
        widget.style = TaskbarWidgetStyle::Glance;
        assert_eq!(resolve(&widget, &limits(0, 0), appearance(false)).rows.len(), 2);
        widget.style = TaskbarWidgetStyle::Rings;
        assert_eq!(resolve(&widget, &limits(0, 0), appearance(false)).rows.len(), 3);
    }

    #[test]
    fn hidden_icon_and_disabled_instances() {
        let provider = ProviderId::primary(ProviderKind::Codex);
        let mut widget = TaskbarWidget::for_provider(provider);
        widget.show_icon = false;
        assert!(resolve(&widget, &limits(0, 0), appearance(false)).icon.is_none());
        let widgets = vec![widget];
        assert!(resolve_visible(&widgets, &[], &limits(0, 0), appearance(false)).is_empty());
        assert_eq!(
            resolve_visible(&widgets, &[provider], &limits(0, 0), appearance(false)).len(),
            1
        );
    }

    #[test]
    fn countdown_is_compact() {
        let now = Utc::now();
        assert_eq!(countdown(now + chrono::Duration::minutes(45), now), "45m");
        assert_eq!(countdown(now + chrono::Duration::minutes(133), now), "2h 13m");
        assert_eq!(countdown(now + chrono::Duration::minutes(3 * 1440 + 240), now), "3d 4h");
    }
}
