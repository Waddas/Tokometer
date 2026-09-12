//! Shared native/window geometry for the two hover toolbars. The frontend uses
//! these exact rectangles, so buttons never extend beyond the transparent window.
use crate::state::Layout;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Top,
    Left,
    Right,
    Bottom,
}

pub fn provider_side() -> Side {
    Side::Right
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Toolbar {
    pub rect: Rect,
    pub columns: usize,
    pub rows: usize,
    pub vertical: bool,
    pub buttons: Vec<Rect>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Geometry {
    pub width: f64,
    pub height: f64,
    pub widget: Rect,
    pub controls: Toolbar,
    pub providers: Toolbar,
}

impl Geometry {
    #[cfg(test)]
    pub fn new(layout: Layout, scale: f64, controls: Side, providers: Side) -> Self {
        Self::with_providers(layout, scale, controls, providers, true)
    }

    pub fn with_providers(layout: Layout, scale: f64, controls: Side, providers: Side, show_providers: bool) -> Self {
        let (w, h) = layout.design_size();
        let (w, h) = (w * scale, h * scale);
        let toolbar = |side: Side, count: usize| {
            let vertical = matches!(side, Side::Left | Side::Right);
            let capacity = (((if vertical { h } else { w }) + 4.0) / 28.0)
                .floor()
                .max(1.0) as usize;
            let along = count.min(capacity);
            let across = count.div_ceil(along);
            let (columns, rows) = if vertical {
                (across, along)
            } else {
                (along, across)
            };
            Toolbar {
                rect: Rect {
                    x: 0.0,
                    y: 0.0,
                    width: columns as f64 * 28.0 - 4.0,
                    height: rows as f64 * 28.0 - 4.0,
                },
                buttons: (0..count).map(|i| Rect {
                    x: (if vertical { i / rows } else { i % columns }) as f64 * 28.0,
                    y: (if vertical { i % rows } else { i / columns }) as f64 * 28.0,
                    width: 24.0, height: 24.0,
                }).collect(),
                columns,
                rows,
                vertical,
            }
        };
        // Keep drag separate in a single row. Wrapped layouts always have
        // drag and close at the top corners, with the other rows centered.
        let mut a = toolbar(controls, 5);
        let horizontal = !a.vertical;
        let (width, height, points): (f64, f64, Vec<(f64, f64)>) =
            if horizontal && w >= 156.0 {
                let width = w - 8.0;
                (width, 24.0, vec![(0.0, 0.0), (width - 108.0, 0.0),
                    (width - 80.0, 0.0), (width - 52.0, 0.0), (width - 24.0, 0.0)])
            } else if (horizontal && w >= 80.0) || (!horizontal && (52.0..80.0).contains(&h)) {
                (80.0, 52.0, vec![(0.0, 0.0), (0.0, 28.0), (28.0, 28.0), (56.0, 28.0), (56.0, 0.0)])
            } else if horizontal || h >= 80.0 {
                (52.0, 80.0, vec![(0.0, 0.0), (0.0, 28.0), (28.0, 28.0), (14.0, 56.0), (28.0, 0.0)])
            } else {
                (148.0, 24.0, vec![(0.0, 0.0), (40.0, 0.0), (68.0, 0.0), (96.0, 0.0), (124.0, 0.0)])
            };
        a.rect.width = width;
        a.rect.height = height;
        a.columns = ((width + 4.0) / 28.0).ceil() as usize;
        a.rows = ((height + 4.0) / 28.0).ceil() as usize;
        a.buttons = points.into_iter().map(|(x, y)| Rect { x, y, width: 24.0, height: 24.0 }).collect();
        let mut b = toolbar(providers, 2);
        if !show_providers { b.rect.width = 0.0; b.rect.height = 0.0; b.buttons.clear(); }
        let mut gutters = [0.0; 4];
        let index = |s: Side| match s {
            Side::Top => 0,
            Side::Right => 1,
            Side::Bottom => 2,
            Side::Left => 3,
        };
        for (side, bar) in [(controls, &a), (providers, &b)] {
            if bar.buttons.is_empty() { continue; }
            gutters[index(side)] += if bar.vertical {
                bar.rect.width + 8.0
            } else {
                bar.rect.height + 8.0
            };
        }
        let widget = Rect {
            x: gutters[3],
            y: gutters[0],
            width: w,
            height: h,
        };
        let mut offsets = [0.0; 4];
        for (side, bar) in [(controls, &mut a), (providers, &mut b)] {
            if bar.buttons.is_empty() { continue; }
            let offset = offsets[index(side)];
            let r = &mut bar.rect;
            match side {
                Side::Top => {
                    r.x = widget.x + (w - r.width) / 2.0;
                    r.y = widget.y - offset - r.height - 4.0;
                }
                Side::Bottom => {
                    r.x = widget.x + (w - r.width) / 2.0;
                    r.y = widget.y + h + offset + 4.0;
                }
                Side::Left => {
                    r.x = widget.x - offset - r.width - 4.0;
                    r.y = widget.y + (h - r.height) / 2.0;
                }
                Side::Right => {
                    r.x = widget.x + w + offset + 4.0;
                    r.y = widget.y + (h - r.height) / 2.0;
                }
            }
            offsets[index(side)] += if bar.vertical {
                r.width + 8.0
            } else {
                r.height + 8.0
            };
        }
        Self {
            width: w + gutters[1] + gutters[3],
            height: h + gutters[0] + gutters[2],
            widget,
            controls: a,
            providers: b,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wrapped_controls_keep_corners_and_center_the_last_row() {
        for scale in [0.5, 2.0 / 3.0, 1.0] {
            let g = Geometry::new(Layout::TilesColumn, scale, Side::Top, Side::Right);
            let bar = g.controls;
            assert_eq!((bar.buttons[0].x, bar.buttons[0].y), (0.0, 0.0));
            assert_eq!((bar.buttons[4].x, bar.buttons[4].y), (bar.rect.width - 24.0, 0.0));
            if scale == 0.5 {
                assert_eq!(bar.buttons[3].x + 12.0, bar.rect.width / 2.0);
                assert_eq!(bar.buttons[3].y, 56.0);
            } else {
                assert_eq!(bar.buttons[2].x + 12.0, bar.rect.width / 2.0);
                assert_eq!(bar.buttons[3].y, 28.0);
            }
        }
        let g = Geometry::with_providers(Layout::MascotLeft, 1.0, Side::Top, Side::Right, false);
        assert!(g.providers.buttons.is_empty());
        assert_eq!(g.width, g.widget.width);
        assert!(g.controls.buttons[1].x - 24.0 >= 16.0);
    }

    #[test]
    fn every_placement_fits_without_overlap_including_narrow_layouts() {
        let sides = [Side::Top, Side::Left, Side::Right, Side::Bottom];
        for layout in Layout::ALL {
            for scale in [0.5, 2.0 / 3.0, 1.0, 2.5] {
                for a in sides {
                    for b in sides {
                        let g = Geometry::new(layout, scale, a, b);
                        let rects = [g.widget, g.controls.rect, g.providers.rect];
                        for r in rects {
                            assert!(r.x >= 0.0 && r.y >= 0.0);
                            assert!(
                                r.x + r.width <= g.width + 0.001
                                    && r.y + r.height <= g.height + 0.001
                            );
                        }
                        for (i, r) in rects.iter().enumerate() {
                            for s in &rects[i + 1..] {
                                assert!(
                                    r.x + r.width <= s.x
                                        || s.x + s.width <= r.x
                                        || r.y + r.height <= s.y
                                        || s.y + s.height <= r.y
                                );
                            }
                        }
                        for bar in [&g.controls, &g.providers] {
                            for (i, r) in bar.buttons.iter().enumerate() {
                                assert!(r.x >= 0.0 && r.y >= 0.0);
                                assert!(r.x + r.width <= bar.rect.width + 0.001);
                                assert!(r.y + r.height <= bar.rect.height + 0.001);
                                for other in &bar.buttons[i + 1..] {
                                    assert!(r.x + r.width <= other.x || other.x + other.width <= r.x
                                        || r.y + r.height <= other.y || other.y + other.height <= r.y);
                                }
                            }
                        }
                        assert!(g.controls.columns * g.controls.rows >= 5);
                        assert!(g.providers.columns * g.providers.rows >= 2);
                    }
                }
            }
        }
    }
}
