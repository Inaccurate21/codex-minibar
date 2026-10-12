//! Direct2D renderer shared by the taskbar host and the Settings previews.
//!
//! Draws into a 32-bit premultiplied DIB, which the host hands to
//! `UpdateLayeredWindow`. Everything is laid out in DIPs; the render target's
//! DPI maps them to device pixels. Geometry and colors follow the Windows 11
//! taskbar: 4 DIP vertical inset, 4 DIP corners, a 6 % white hover fill with
//! a lighter top edge, Segoe UI Variable text and WinUI progress styling.

#![cfg(windows)]

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use windows::{
    Win32::{
        Foundation::RECT,
        Graphics::{
            Direct2D::{Common::*, *},
            DirectWrite::*,
            Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM,
            Gdi::HDC,
        },
    },
    core::{HSTRING, PCWSTR, w},
};
use windows_numerics::Vector2;

use super::model::{Appearance, Icon, Rgb, WidgetView};
use crate::settings::TaskbarWidgetStyle;

/// Vertical inset of the hover plate inside the taskbar.
const INSET_Y: f32 = 4.0;
const RADIUS: f32 = 4.0;
const PAD_START: f32 = 8.0;
const PAD_END: f32 = 10.0;
const ICON: f32 = 20.0;
const ICON_GAP: f32 = 8.0;
const LABEL_GAP: f32 = 4.0;
const RING: f32 = 28.0;
const RING_GAP: f32 = 6.0;
const UNIT_GAP: f32 = 12.0;
const BAR_SINGLE: f32 = 72.0;
const BAR_MULTI: f32 = 44.0;
const BAR_THICKNESS: f32 = 3.0;
const MAX_LABEL_CHARS: usize = 18;

/// Device geometry of the strip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Metrics {
    /// Device pixels per DIP.
    pub(crate) scale: f32,
    /// Taskbar height in DIPs.
    pub(crate) height: f32,
}

impl Metrics {
    fn inner(self) -> f32 {
        (self.height - INSET_Y * 2.0).max(16.0)
    }

    /// Font size of the two-line layouts; 12 DIP like the clock, smaller on
    /// the compact taskbar.
    fn line_font(self) -> f32 {
        (self.inner() / 2.0 / 1.36).clamp(8.5, 12.0)
    }

    fn snap(self, value: f32) -> f32 {
        (value * self.scale).round() / self.scale
    }
}

/// One widget placed on the strip, with its animation state.
pub(crate) struct Item<'a> {
    pub(crate) view: &'a WidgetView,
    pub(crate) x: f32,
    pub(crate) width: f32,
    pub(crate) hover: f32,
    pub(crate) press: f32,
    pub(crate) opacity: f32,
    /// Animated fill per row; `None` rows draw only the track.
    pub(crate) progress: &'a [Option<f32>],
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Weight {
    Regular,
    Semibold,
}

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    Primary,
    Secondary,
    Tint(Rgb),
}

enum Prim {
    Text {
        layout: IDWriteTextLayout,
        x: f32,
        y: f32,
        tone: Tone,
    },
    Icon {
        icon: Icon,
        x: f32,
        y: f32,
        size: f32,
    },
    Bar {
        x: f32,
        y: f32,
        width: f32,
        row: usize,
        color: Rgb,
    },
    Ring {
        cx: f32,
        cy: f32,
        radius: f32,
        stroke: f32,
        row: usize,
        color: Rgb,
    },
}

struct Content {
    width: f32,
    prims: Vec<Prim>,
}

struct Line {
    layout: IDWriteTextLayout,
    width: f32,
    height: f32,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct IconKey {
    name: &'static str,
    pixels: u32,
    tint: Rgb,
}

pub(crate) struct Renderer {
    factory: ID2D1Factory,
    dwrite: IDWriteFactory,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    round: ID2D1StrokeStyle,
    family: HSTRING,
    formats: HashMap<(u32, Weight), IDWriteTextFormat>,
    icons: HashMap<IconKey, Option<ID2D1Bitmap>>,
    dpi: f32,
}

fn color(rgb: Rgb, alpha: f32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: f32::from(rgb[0]) / 255.0,
        g: f32::from(rgb[1]) / 255.0,
        b: f32::from(rgb[2]) / 255.0,
        a: alpha.clamp(0.0, 1.0),
    }
}

fn rect(left: f32, top: f32, right: f32, bottom: f32) -> D2D_RECT_F {
    D2D_RECT_F {
        left,
        top,
        right,
        bottom,
    }
}

fn truncate(label: &str) -> String {
    if label.chars().count() <= MAX_LABEL_CHARS {
        return label.to_owned();
    }
    let mut short = label
        .chars()
        .take(MAX_LABEL_CHARS - 1)
        .collect::<String>()
        .trim_end()
        .to_owned();
    short.push('\u{2026}');
    short
}

impl Renderer {
    pub(crate) fn new() -> windows::core::Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let (target, brush) = Self::create_target(&factory, 96.0)?;
            let round = factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    dashStyle: D2D1_DASH_STYLE_SOLID,
                    dashOffset: 0.0,
                },
                None,
            )?;
            let family = Self::resolve_family(&dwrite);
            Ok(Self {
                factory,
                dwrite,
                target,
                brush,
                round,
                family,
                formats: HashMap::new(),
                icons: HashMap::new(),
                dpi: 96.0,
            })
        }
    }

    fn create_target(
        factory: &ID2D1Factory,
        dpi: f32,
    ) -> windows::core::Result<(ID2D1DCRenderTarget, ID2D1SolidColorBrush)> {
        unsafe {
            let target = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_SOFTWARE,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: dpi,
                dpiY: dpi,
                usage: D2D1_RENDER_TARGET_USAGE_GDI_COMPATIBLE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            })?;
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target.CreateSolidColorBrush(&color([255, 255, 255], 1.0), None)?;
            Ok((target, brush))
        }
    }

    /// Segoe UI Variable Text on Windows 11, Segoe UI before it.
    fn resolve_family(dwrite: &IDWriteFactory) -> HSTRING {
        unsafe {
            let mut collection = None;
            if dwrite
                .GetSystemFontCollection(&mut collection, false)
                .is_ok()
                && let Some(collection) = collection
            {
                let mut index = 0;
                let mut exists = windows::core::BOOL(0);
                if collection
                    .FindFamilyName(w!("Segoe UI Variable Text"), &mut index, &mut exists)
                    .is_ok()
                    && exists.as_bool()
                {
                    return HSTRING::from("Segoe UI Variable Text");
                }
            }
        }
        HSTRING::from("Segoe UI")
    }

    fn set_dpi(&mut self, scale: f32) -> windows::core::Result<()> {
        let dpi = 96.0 * scale;
        if (dpi - self.dpi).abs() > f32::EPSILON {
            unsafe { self.target.SetDpi(dpi, dpi) };
            self.dpi = dpi;
            // Icons are rasterized at device size.
            self.icons.clear();
        }
        Ok(())
    }

    fn recreate(&mut self) -> windows::core::Result<()> {
        let (target, brush) = Self::create_target(&self.factory, self.dpi)?;
        self.target = target;
        self.brush = brush;
        self.icons.clear();
        Ok(())
    }

    fn format(&mut self, size: f32, weight: Weight) -> windows::core::Result<IDWriteTextFormat> {
        let key = ((size * 100.0).round() as u32, weight);
        if let Some(format) = self.formats.get(&key) {
            return Ok(format.clone());
        }
        let format = unsafe {
            let format = self.dwrite.CreateTextFormat(
                PCWSTR(self.family.as_ptr()),
                None,
                match weight {
                    Weight::Regular => DWRITE_FONT_WEIGHT_NORMAL,
                    Weight::Semibold => DWRITE_FONT_WEIGHT_SEMI_BOLD,
                },
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!(""),
            )?;
            format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP).ok()?;
            format
        };
        self.formats.insert(key, format.clone());
        Ok(format)
    }

    fn line(&mut self, text: &str, size: f32, weight: Weight) -> windows::core::Result<Line> {
        let format = self.format(size, weight)?;
        let wide = text.encode_utf16().collect::<Vec<_>>();
        unsafe {
            let layout = self
                .dwrite
                .CreateTextLayout(&wide, &format, 4096.0, 256.0)?;
            let mut metrics = DWRITE_TEXT_METRICS::default();
            layout.GetMetrics(&mut metrics).ok()?;
            Ok(Line {
                layout,
                width: metrics.widthIncludingTrailingWhitespace,
                height: metrics.height,
            })
        }
    }

    fn icon_bitmap(&mut self, icon: &Icon, size: f32) -> Option<ID2D1Bitmap> {
        let pixels = (size * self.dpi / 96.0).round().max(1.0) as u32;
        let key = IconKey {
            name: icon.name,
            pixels,
            tint: icon.tint,
        };
        if let Some(bitmap) = self.icons.get(&key) {
            return bitmap.clone();
        }
        let bitmap = rasterize_icon(icon.name, pixels, icon.tint).and_then(|data| unsafe {
            self.target
                .CreateBitmap(
                    D2D_SIZE_U {
                        width: pixels,
                        height: pixels,
                    },
                    Some(data.as_ptr().cast()),
                    pixels * 4,
                    &D2D1_BITMAP_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_B8G8R8A8_UNORM,
                            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                    },
                )
                .ok()
        });
        self.icons.insert(key, bitmap.clone());
        bitmap
    }

    /// Width of a widget in DIPs.
    pub(crate) fn measure(
        &mut self,
        view: &WidgetView,
        metrics: Metrics,
        now: DateTime<Utc>,
    ) -> windows::core::Result<f32> {
        self.set_dpi(metrics.scale)?;
        Ok(metrics.snap(self.content(view, metrics, now)?.width))
    }

    fn content(
        &mut self,
        view: &WidgetView,
        metrics: Metrics,
        now: DateTime<Utc>,
    ) -> windows::core::Result<Content> {
        match view.style {
            TaskbarWidgetStyle::Glance => self.glance(view, metrics, now),
            TaskbarWidgetStyle::Rings => self.rings(view, metrics),
            TaskbarWidgetStyle::Bars => self.bars(view, metrics),
            TaskbarWidgetStyle::Compact => self.compact(view, metrics),
        }
    }

    fn icon_size(metrics: Metrics) -> f32 {
        ICON.min(metrics.inner() - 12.0).max(12.0)
    }

    /// Leading provider mark; returns the x where content continues.
    fn leading_icon(view: &WidgetView, metrics: Metrics, prims: &mut Vec<Prim>) -> f32 {
        let Some(icon) = view.icon.clone() else {
            return PAD_START + 2.0;
        };
        let size = Self::icon_size(metrics);
        prims.push(Prim::Icon {
            icon,
            x: PAD_START,
            y: (metrics.height - size) / 2.0,
            size,
        });
        PAD_START + size + ICON_GAP
    }

    /// Stacks lines vertically centered in the taskbar, left-aligned at `x`
    /// (or right-aligned to `x + block` when `right` is set).
    fn stack(
        lines: Vec<Vec<(Line, Tone)>>,
        x: f32,
        metrics: Metrics,
        right: bool,
        prims: &mut Vec<Prim>,
    ) -> f32 {
        let widths = lines
            .iter()
            .map(|runs| {
                runs.iter().map(|(line, _)| line.width).sum::<f32>()
                    + LABEL_GAP * runs.len().saturating_sub(1) as f32
            })
            .collect::<Vec<_>>();
        let block = widths.iter().copied().fold(0.0, f32::max);
        let heights = lines
            .iter()
            .map(|runs| runs.iter().map(|(line, _)| line.height).fold(0.0, f32::max))
            .collect::<Vec<_>>();
        let total = heights.iter().sum::<f32>();
        let mut y = (metrics.height - total) / 2.0;
        for ((runs, width), height) in lines.into_iter().zip(widths).zip(heights) {
            let mut cursor = if right { x + block - width } else { x };
            for (line, tone) in runs {
                let advance = line.width + LABEL_GAP;
                prims.push(Prim::Text {
                    // Baselines of mixed sizes share the bottom edge.
                    y: y + height - line.height,
                    layout: line.layout,
                    x: cursor,
                    tone,
                });
                cursor += advance;
            }
            y += height;
        }
        block
    }

    fn value_tone(row: &super::model::Row) -> Tone {
        if row.progress.is_none() {
            Tone::Secondary
        } else if row.tinted {
            Tone::Tint(row.color)
        } else {
            Tone::Primary
        }
    }

    fn glance(
        &mut self,
        view: &WidgetView,
        metrics: Metrics,
        now: DateTime<Utc>,
    ) -> windows::core::Result<Content> {
        let mut prims = Vec::new();
        let x = Self::leading_icon(view, metrics, &mut prims);
        let font = metrics.line_font();
        let lines = if let [row] = view.rows.as_slice() {
            vec![
                vec![(self.line(&row.value, font, Weight::Semibold)?, Self::value_tone(row))],
                vec![(
                    self.line(&truncate(&view.glance_caption(now)), font, Weight::Regular)?,
                    Tone::Secondary,
                )],
            ]
        } else {
            let mut lines = Vec::new();
            for row in &view.rows {
                lines.push(vec![
                    (self.line(&truncate(&row.label), font, Weight::Regular)?, Tone::Secondary),
                    (self.line(&row.value, font, Weight::Semibold)?, Self::value_tone(row)),
                ]);
            }
            lines
        };
        let block = Self::stack(lines, x, metrics, false, &mut prims);
        Ok(Content {
            width: x + block + PAD_END,
            prims,
        })
    }

    fn compact(&mut self, view: &WidgetView, metrics: Metrics) -> windows::core::Result<Content> {
        let mut prims = Vec::new();
        let font = metrics.line_font();
        let lines = if let [row] = view.rows.as_slice() {
            vec![
                vec![(self.line(&row.value, font, Weight::Regular)?, Self::value_tone(row))],
                vec![(self.line(&truncate(&row.label), font, Weight::Regular)?, Tone::Secondary)],
            ]
        } else {
            let mut lines = Vec::new();
            for row in &view.rows {
                lines.push(vec![
                    (self.line(&truncate(&row.label), font, Weight::Regular)?, Tone::Secondary),
                    (self.line(&row.value, font, Weight::Regular)?, Self::value_tone(row)),
                ]);
            }
            lines
        };
        let block = Self::stack(lines, PAD_START + 2.0, metrics, true, &mut prims);
        Ok(Content {
            width: PAD_START + 2.0 + block + PAD_START + 2.0,
            prims,
        })
    }

    fn bars(&mut self, view: &WidgetView, metrics: Metrics) -> windows::core::Result<Content> {
        let mut prims = Vec::new();
        let x = Self::leading_icon(view, metrics, &mut prims);
        if let [row] = view.rows.as_slice() {
            let font = metrics.line_font();
            let label = self.line(&truncate(&row.label), font - 1.0, Weight::Regular)?;
            let value = self.line(&row.value, font, Weight::Semibold)?;
            let width = BAR_SINGLE.max(label.width + value.width + 12.0);
            let gap = 5.0;
            let total = value.height + gap + BAR_THICKNESS;
            let top = (metrics.height - total) / 2.0;
            prims.push(Prim::Text {
                x,
                y: top + value.height - label.height,
                layout: label.layout,
                tone: Tone::Secondary,
            });
            prims.push(Prim::Text {
                x: x + width - value.width,
                y: top,
                layout: value.layout,
                tone: Self::value_tone(row),
            });
            prims.push(Prim::Bar {
                x,
                y: top + value.height + gap + BAR_THICKNESS / 2.0,
                width,
                row: 0,
                color: row.color,
            });
            return Ok(Content {
                width: x + width + PAD_END,
                prims,
            });
        }
        let count = view.rows.len().max(1) as f32;
        let font = if view.rows.len() > 2 {
            (metrics.inner() / count / 1.3).clamp(8.0, 10.0)
        } else {
            (metrics.line_font() - 1.0).max(8.0)
        };
        let mut labels = Vec::new();
        let mut values = Vec::new();
        for row in &view.rows {
            labels.push(self.line(&truncate(&row.label), font, Weight::Regular)?);
            values.push(self.line(&row.value, font, Weight::Semibold)?);
        }
        let label_width = labels.iter().map(|line| line.width).fold(0.0, f32::max);
        let value_width = values.iter().map(|line| line.width).fold(0.0, f32::max);
        let pitch = labels
            .iter()
            .map(|line| line.height)
            .fold(0.0, f32::max)
            .min((metrics.inner() - 2.0) / count);
        let top = (metrics.height - pitch * count) / 2.0;
        let bar_x = x + label_width + 6.0;
        let value_x = bar_x + BAR_MULTI + 6.0;
        for (index, ((label, value), row)) in labels
            .into_iter()
            .zip(values)
            .zip(&view.rows)
            .enumerate()
        {
            let center = top + pitch * (index as f32 + 0.5);
            prims.push(Prim::Text {
                x,
                y: center - label.height / 2.0,
                layout: label.layout,
                tone: Tone::Secondary,
            });
            prims.push(Prim::Bar {
                x: bar_x,
                y: center,
                width: BAR_MULTI,
                row: index,
                color: row.color,
            });
            prims.push(Prim::Text {
                x: value_x + value_width - value.width,
                y: center - value.height / 2.0,
                layout: value.layout,
                tone: Self::value_tone(row),
            });
        }
        Ok(Content {
            width: value_x + value_width + PAD_END,
            prims,
        })
    }

    fn rings(&mut self, view: &WidgetView, metrics: Metrics) -> windows::core::Result<Content> {
        let mut prims = Vec::new();
        let diameter = RING.min(metrics.inner() - 8.0).max(16.0);
        let stroke = if diameter >= 24.0 { 3.0 } else { 2.5 };
        let font = metrics.line_font();
        let mut x = PAD_START;
        for (index, row) in view.rows.iter().enumerate() {
            if index > 0 {
                x += UNIT_GAP;
            }
            let cx = x + diameter / 2.0;
            let cy = metrics.height / 2.0;
            prims.push(Prim::Ring {
                cx,
                cy,
                radius: (diameter - stroke) / 2.0,
                stroke,
                row: index,
                color: row.color,
            });
            if view.icon.is_some() {
                let size = (diameter * 0.46).round();
                prims.push(Prim::Icon {
                    icon: row.icon.clone(),
                    x: cx - size / 2.0,
                    y: cy - size / 2.0,
                    size,
                });
            }
            x += diameter + RING_GAP;
            let lines = vec![
                vec![(self.line(&row.value, font, Weight::Semibold)?, Self::value_tone(row))],
                vec![(self.line(&truncate(&row.label), font - 1.0, Weight::Regular)?, Tone::Secondary)],
            ];
            x += Self::stack(lines, x, metrics, false, &mut prims);
        }
        Ok(Content {
            width: x + PAD_END,
            prims,
        })
    }

    /// Paints the strip into a DIB selected into `hdc`.
    pub(crate) fn paint(
        &mut self,
        hdc: *mut core::ffi::c_void,
        size: (i32, i32),
        metrics: Metrics,
        appearance: Appearance,
        items: &[Item<'_>],
        now: DateTime<Utc>,
    ) -> windows::core::Result<()> {
        self.set_dpi(metrics.scale)?;
        let result = self.paint_once(hdc, size, metrics, appearance, items, now);
        if let Err(error) = &result
            && error.code() == windows::Win32::Foundation::D2DERR_RECREATE_TARGET
        {
            self.recreate()?;
            return self.paint_once(hdc, size, metrics, appearance, items, now);
        }
        result
    }

    fn paint_once(
        &mut self,
        hdc: *mut core::ffi::c_void,
        size: (i32, i32),
        metrics: Metrics,
        appearance: Appearance,
        items: &[Item<'_>],
        now: DateTime<Utc>,
    ) -> windows::core::Result<()> {
        let bounds = RECT {
            left: 0,
            top: 0,
            right: size.0,
            bottom: size.1,
        };
        // Build every text layout before drawing begins.
        let mut contents = Vec::with_capacity(items.len());
        for item in items {
            contents.push(self.content(item.view, metrics, now)?);
        }
        unsafe {
            self.target.BindDC(HDC(hdc), &bounds).ok()?;
            self.target.BeginDraw();
            self.target.Clear(Some(&color([0, 0, 0], 0.0)));
        }
        for (item, content) in items.iter().zip(contents) {
            self.paint_item(item, content, metrics, appearance);
        }
        unsafe { self.target.EndDraw(None, None).ok() }
    }

    fn fill(&self, shape: &D2D1_ROUNDED_RECT, rgb: Rgb, alpha: f32) {
        unsafe {
            self.brush.SetColor(&color(rgb, alpha));
            self.target.FillRoundedRectangle(shape, &self.brush);
        }
    }

    fn paint_item(&mut self, item: &Item<'_>, content: Content, metrics: Metrics, appearance: Appearance) {
        let x = metrics.snap(item.x);
        let plate = D2D1_ROUNDED_RECT {
            rect: rect(x, INSET_Y, x + item.width, metrics.height - INSET_Y),
            radiusX: RADIUS,
            radiusY: RADIUS,
        };
        let opacity = item.opacity.clamp(0.0, 1.0);
        // Layered windows let clicks fall through fully transparent pixels;
        // the whole plate stays hit-testable without becoming visible.
        self.fill(&plate, [255, 255, 255], 1.0 / 255.0);
        self.paint_plate(&plate, item.hover, item.press, opacity, appearance);
        // Pressed content dims like a taskbar button.
        let content_opacity = opacity * (1.0 - 0.2 * item.press);
        unsafe {
            let translate = windows_numerics::Matrix3x2::translation(x, 0.0);
            self.target.SetTransform(&translate);
        }
        for prim in content.prims {
            self.paint_prim(prim, item.progress, metrics, appearance, content_opacity);
        }
        unsafe {
            self.target
                .SetTransform(&windows_numerics::Matrix3x2::identity());
        }
    }

    fn paint_plate(
        &self,
        plate: &D2D1_ROUNDED_RECT,
        hover: f32,
        press: f32,
        opacity: f32,
        appearance: Appearance,
    ) {
        let amount = hover.max(press) * opacity;
        if amount <= 0.001 {
            return;
        }
        let (fill, edge, edge_alpha) = if appearance.light {
            // Light taskbar buttons brighten toward white with a hairline.
            ([255, 255, 255], [0, 0, 0], 0.07)
        } else {
            ([255, 255, 255], [255, 255, 255], 0.045)
        };
        let fill_alpha = match (appearance.light, press > 0.5) {
            (false, false) => 0.0605,
            (false, true) => 0.04,
            (true, false) => 0.7,
            (true, true) => 0.5,
        };
        self.fill(plate, fill, fill_alpha * amount);
        // Elevation edge: a lighter top in dark mode, a darker bottom in light.
        unsafe {
            let top = plate.rect.top;
            let bottom = plate.rect.bottom;
            let (start, end) = if appearance.light {
                (bottom, bottom - 3.0)
            } else {
                (top, top + 3.0)
            };
            let stops = [
                D2D1_GRADIENT_STOP {
                    position: 0.0,
                    color: color(edge, edge_alpha * amount),
                },
                D2D1_GRADIENT_STOP {
                    position: 1.0,
                    color: color(edge, 0.0),
                },
            ];
            let Ok(collection) = self.target.CreateGradientStopCollection(
                &stops,
                D2D1_GAMMA_2_2,
                D2D1_EXTEND_MODE_CLAMP,
            ) else {
                return;
            };
            let Ok(gradient) = self.target.CreateLinearGradientBrush(
                &D2D1_LINEAR_GRADIENT_BRUSH_PROPERTIES {
                    startPoint: Vector2 { x: 0.0, y: start },
                    endPoint: Vector2 { x: 0.0, y: end },
                },
                None,
                &collection,
            ) else {
                return;
            };
            let half = 0.5 / (self.dpi / 96.0);
            let edge_rect = D2D1_ROUNDED_RECT {
                rect: rect(
                    plate.rect.left + half,
                    plate.rect.top + half,
                    plate.rect.right - half,
                    plate.rect.bottom - half,
                ),
                radiusX: RADIUS - half,
                radiusY: RADIUS - half,
            };
            self.target
                .DrawRoundedRectangle(&edge_rect, &gradient, half * 2.0, None);
        }
    }

    fn tone_color(tone: Tone, appearance: Appearance) -> (Rgb, f32) {
        match tone {
            Tone::Primary => (appearance.text(), appearance.text_alpha()),
            Tone::Secondary => (appearance.text(), appearance.secondary_alpha()),
            Tone::Tint(rgb) => (rgb, 1.0),
        }
    }

    fn track_alpha(appearance: Appearance) -> f32 {
        // WinUI ProgressBar track: ControlStrongStrokeColorDefault.
        if appearance.light { 0.446 } else { 0.545 }
    }

    fn paint_prim(
        &mut self,
        prim: Prim,
        progress: &[Option<f32>],
        metrics: Metrics,
        appearance: Appearance,
        opacity: f32,
    ) {
        match prim {
            Prim::Text { layout, x, y, tone } => {
                let (rgb, alpha) = Self::tone_color(tone, appearance);
                unsafe {
                    self.brush.SetColor(&color(rgb, alpha * opacity));
                    self.target.DrawTextLayout(
                        Vector2 {
                            x: metrics.snap(x),
                            y: metrics.snap(y),
                        },
                        &layout,
                        &self.brush,
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                    );
                }
            }
            Prim::Icon { icon, x, y, size } => {
                if let Some(bitmap) = self.icon_bitmap(&icon, size) {
                    let (x, y) = (metrics.snap(x), metrics.snap(y));
                    let device = (size * metrics.scale).round() / metrics.scale;
                    unsafe {
                        self.target.DrawBitmap(
                            &bitmap,
                            Some(&rect(x, y, x + device, y + device)),
                            opacity,
                            D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                            None,
                        );
                    }
                }
            }
            Prim::Bar {
                x,
                y,
                width,
                row,
                color: rgb,
            } => unsafe {
                let one = 1.0 / metrics.scale;
                let y = metrics.snap(y);
                self.brush.SetColor(&color(
                    appearance.text(),
                    Self::track_alpha(appearance) * opacity,
                ));
                self.target.FillRectangle(
                    &rect(x, y - one / 2.0, x + width, y + one / 2.0),
                    &self.brush,
                );
                if let Some(fill) = progress.get(row).copied().flatten()
                    && fill > 0.004
                {
                    let half = BAR_THICKNESS / 2.0;
                    let length = (width * fill.clamp(0.0, 1.0)).max(BAR_THICKNESS);
                    self.fill(
                        &D2D1_ROUNDED_RECT {
                            rect: rect(x, y - half, x + length, y + half),
                            radiusX: half,
                            radiusY: half,
                        },
                        rgb,
                        opacity,
                    );
                }
            },
            Prim::Ring {
                cx,
                cy,
                radius,
                stroke,
                row,
                color: rgb,
            } => unsafe {
                let one = 1.0 / metrics.scale;
                let center = Vector2 { x: cx, y: cy };
                self.brush.SetColor(&color(
                    appearance.text(),
                    Self::track_alpha(appearance) * opacity,
                ));
                self.target.DrawEllipse(
                    &D2D1_ELLIPSE {
                        point: center,
                        radiusX: radius,
                        radiusY: radius,
                    },
                    &self.brush,
                    one,
                    None,
                );
                let Some(fill) = progress.get(row).copied().flatten() else {
                    return;
                };
                let fill = fill.clamp(0.0, 1.0);
                if fill <= 0.004 {
                    return;
                }
                self.brush.SetColor(&color(rgb, opacity));
                if fill >= 0.999 {
                    self.target.DrawEllipse(
                        &D2D1_ELLIPSE {
                            point: center,
                            radiusX: radius,
                            radiusY: radius,
                        },
                        &self.brush,
                        stroke,
                        &self.round,
                    );
                    return;
                }
                if let Some(arc) = self.arc(center, radius, fill) {
                    self.target
                        .DrawGeometry(&arc, &self.brush, stroke, &self.round);
                }
            },
        }
    }

    /// Clockwise arc from twelve o'clock covering `fill` of the circle.
    fn arc(&self, center: Vector2, radius: f32, fill: f32) -> Option<ID2D1PathGeometry> {
        let point = |turn: f32| {
            let angle = turn * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
            Vector2 {
                x: center.x + radius * angle.cos(),
                y: center.y + radius * angle.sin(),
            }
        };
        unsafe {
            let path = self.factory.CreatePathGeometry().ok()?;
            let sink = path.Open().ok()?;
            sink.BeginFigure(point(0.0), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: point(fill),
                size: D2D_SIZE_F {
                    width: radius,
                    height: radius,
                },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: if fill > 0.5 {
                    D2D1_ARC_SIZE_LARGE
                } else {
                    D2D1_ARC_SIZE_SMALL
                },
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close().ok().ok()?;
            Some(path)
        }
    }
}

/// Premultiplied BGRA alpha mask of a monochrome icon, tinted.
fn rasterize_icon(name: &str, pixels: u32, tint: Rgb) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let source = crate::popup_window::ui::assets::svg_source(name)?;
    let svg = crate::popup_window::ui::assets::normalize_svg(source);
    let tree = usvg::Tree::from_str(&svg, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = pixels as f32 / size.width().max(size.height());
    let mut pixmap = tiny_skia::Pixmap::new(pixels, pixels)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(
        pixmap
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let alpha = u16::from(pixel.alpha());
                let channel = |value: u8| ((u16::from(value) * alpha + 127) / 255) as u8;
                [channel(tint[2]), channel(tint[1]), channel(tint[0]), pixel.alpha()]
            })
            .collect(),
    )
}

/// Straight-alpha RGBA rendering of widgets, for previews in Settings.
pub(crate) struct Snapshot {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// Renders `views` side by side the way the taskbar host lays them out.
pub(crate) fn snapshot(
    renderer: &mut Renderer,
    views: &[WidgetView],
    metrics: Metrics,
    appearance: Appearance,
    hovered: Option<usize>,
) -> Option<Snapshot> {
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, SelectObject,
    };
    let now = Utc::now();
    let mut widths = Vec::with_capacity(views.len());
    for view in views {
        widths.push(renderer.measure(view, metrics, now).ok()?);
    }
    let gap = super::WIDGET_GAP;
    let total = widths.iter().sum::<f32>() + gap * widths.len().saturating_sub(1) as f32;
    let width = (total * metrics.scale).ceil().max(1.0) as i32;
    let height = (metrics.height * metrics.scale).round().max(1.0) as i32;
    let progress = views
        .iter()
        .map(|view| view.rows.iter().map(|row| row.progress).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let mut x = 0.0;
    let items = views
        .iter()
        .zip(&widths)
        .zip(&progress)
        .enumerate()
        .map(|(index, ((view, width), progress))| {
            let item = Item {
                view,
                x,
                width: *width,
                hover: if hovered == Some(index) { 1.0 } else { 0.0 },
                press: 0.0,
                opacity: 1.0,
                progress,
            };
            x += width + gap;
            item
        })
        .collect::<Vec<_>>();
    unsafe {
        let dc = CreateCompatibleDC(std::ptr::null_mut());
        if dc.is_null() {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = width;
        info.bmiHeader.biHeight = -height;
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;
        let mut bits = std::ptr::null_mut();
        let dib = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        if dib.is_null() || bits.is_null() {
            DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, dib);
        let painted = renderer
            .paint(dc, (width, height), metrics, appearance, &items, now)
            .is_ok();
        let pixels = std::slice::from_raw_parts(bits as *const u8, (width * height * 4) as usize);
        let rgba = painted.then(|| {
            pixels
                .chunks_exact(4)
                .flat_map(|pixel| {
                    let alpha = pixel[3];
                    let straight = |value: u8| {
                        if alpha == 0 {
                            0
                        } else {
                            ((u16::from(value) * 255 + u16::from(alpha) / 2) / u16::from(alpha))
                                .min(255) as u8
                        }
                    };
                    [straight(pixel[2]), straight(pixel[1]), straight(pixel[0]), alpha]
                })
                .collect::<Vec<_>>()
        });
        SelectObject(dc, previous);
        DeleteObject(dib);
        DeleteDC(dc);
        rgba.map(|rgba| Snapshot {
            width: width as u32,
            height: height as u32,
            rgba,
        })
    }
}
