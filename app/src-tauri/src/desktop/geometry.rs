//! Shared desktop canvas, DPI conversion and complete drawing envelopes.
use serde::{Deserialize, Serialize};
use tauri::{PhysicalPosition, PhysicalSize};

#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}
impl Rect {
    pub(super) fn contains(self, x: f64, y: f64) -> bool {
        x >= self.left
            && y >= self.top
            && x <= self.left + self.width
            && y <= self.top + self.height
    }
}
#[derive(Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Serialize)]
pub struct Surface {
    pub(super) left: f64,
    pub(super) top: f64,
    pub(super) width: f64,
    pub(super) height: f64,
    pub(super) scale: f64,
}
#[derive(Clone)]
pub(super) struct Canvas {
    pub(super) origin: PhysicalPosition<i32>,
    pub(super) size: PhysicalSize<u32>,
    pub(super) scale: f64,
    pub(super) work_area: Rect,
    pub(super) work_areas: Vec<Rect>,
}
impl Canvas {
    pub(super) fn point(&self, physical: Point) -> Point {
        Point {
            x: (physical.x - self.origin.x as f64) / self.scale,
            y: (physical.y - self.origin.y as f64) / self.scale,
        }
    }
    pub(super) fn physical(&self, point: Point) -> Point {
        Point {
            x: self.origin.x as f64 + point.x * self.scale,
            y: self.origin.y as f64 + point.y * self.scale,
        }
    }
    pub(super) fn surface(&self, window: &tauri::WebviewWindow) -> Result<Surface, String> {
        let origin = window
            .inner_position()
            .map_err(|_| "错误：无法定位手牌窗口")?;
        let scale = window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?;
        let point = self.point(Point {
            x: origin.x as f64,
            y: origin.y as f64,
        });
        Ok(Surface {
            left: point.x,
            top: point.y,
            width: self.size.width as f64 / self.scale,
            height: self.size.height as f64 / self.scale,
            scale: self.scale / scale,
        })
    }
}
pub(super) async fn fit_card_surface(
    window: &tauri::WebviewWindow,
    canvas: &Canvas,
    bounds: Rect,
    keep_origin: bool,
) -> Result<(), String> {
    let from = canvas.physical(Point {
        x: bounds.left - 14.0,
        y: bounds.top - 14.0,
    });
    let to = canvas.physical(Point {
        x: bounds.left + bounds.width + 14.0,
        y: bounds.top + bounds.height + 14.0,
    });
    let anchor = if keep_origin {
        Some(
            window
                .inner_position()
                .map_err(|_| "错误：无法定位手牌窗口")?,
        )
    } else {
        None
    };
    let (origin, size) = drawing_surface_geometry(from, to, anchor);
    // The reply must describe the applied position, not a queued move.
    super::floating_window::place_confirmed(window, origin, size).await?;
    let surface = canvas.surface(window)?;
    // Clip the complete drawing envelope, not the IPC-delayed card rectangles.
    // This trims unused space when the visible window keeps its original anchor.
    apply_regions(
        window,
        &[Rect {
            left: (bounds.left - surface.left) * surface.scale,
            top: (bounds.top - surface.top) * surface.scale,
            width: bounds.width * surface.scale,
            height: bounds.height * surface.scale,
        }],
        false,
    )
}

fn drawing_surface_geometry(
    from: Point,
    to: Point,
    anchor: Option<PhysicalPosition<i32>>,
) -> (PhysicalPosition<i32>, PhysicalSize<u32>) {
    let mut origin = PhysicalPosition::new(from.x.floor() as i32, from.y.floor() as i32);
    if let Some(anchor) = anchor {
        // Shrink the right/bottom extent without moving the visible cards' coordinate origin.
        origin.x = origin.x.min(anchor.x);
        origin.y = origin.y.min(anchor.y);
    }
    let size = PhysicalSize::new(
        (to.x.ceil() - origin.x as f64).max(1.0) as u32,
        (to.y.ceil() - origin.y as f64).max(1.0) as u32,
    );
    (origin, size)
}

/// Dock regions follow its compact shape; cards use their complete animation
/// envelope. The pointer observer independently controls click-through.
pub(super) fn apply_regions(
    window: &tauri::WebviewWindow,
    rects: &[Rect],
    full_surface: bool,
) -> Result<(), String> {
    use windows_sys::Win32::Graphics::Gdi::{
        CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, RGN_OR,
    };
    let hwnd = window.hwnd().map_err(|_| "错误：手牌窗口不可用")?.0;
    unsafe {
        if full_surface {
            SetWindowRgn(hwnd, std::ptr::null_mut(), 1);
            return Ok(());
        }
        let scale = window
            .scale_factor()
            .map_err(|_| "错误：无法读取显示器信息")?;
        let region = CreateRectRgn(0, 0, 0, 0);
        if region.is_null() {
            return Err("错误：无法设置手牌区域".into());
        }
        for rect in rects {
            // Include the card's soft shadow and the short distance traversed between frames.
            let part = CreateRectRgn(
                ((rect.left - 14.0) * scale).floor() as i32,
                ((rect.top - 14.0) * scale).floor() as i32,
                ((rect.left + rect.width + 14.0) * scale).ceil() as i32,
                ((rect.top + rect.height + 14.0) * scale).ceil() as i32,
            );
            CombineRgn(region, region, part, RGN_OR);
            DeleteObject(part);
        }
        if SetWindowRgn(hwnd, region, 1) == 0 {
            DeleteObject(region);
            return Err("错误：无法设置手牌区域".into());
        }
        // Windows owns the region after a successful SetWindowRgn.
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canvas_mapping_keeps_physical_points_across_scale_and_origin_changes() {
        let old = Canvas {
            origin: PhysicalPosition::new(-1920, -120),
            scale: 1.5,
            size: PhysicalSize::new(3840, 2280),
            work_area: Rect::default(),
            work_areas: vec![],
        };
        let new = Canvas {
            origin: PhysicalPosition::new(0, 0),
            scale: 1.0,
            size: PhysicalSize::new(1920, 1080),
            work_area: Rect::default(),
            work_areas: vec![],
        };
        let point = Point {
            x: 1500.0,
            y: 200.0,
        };
        let mapped = new.point(old.physical(point));
        assert_eq!((mapped.x, mapped.y), (330.0, 180.0));
        let physical = new.physical(mapped);
        let before = old.physical(point);
        assert_eq!((physical.x, physical.y), (before.x, before.y));
    }
    #[test]
    fn drawing_surface_keeps_its_visible_origin_when_the_arrival_finishes() {
        let from = Point {
            x: -122.2,
            y: 270.4,
        };
        let to = Point { x: 301.7, y: 800.2 };
        let anchor = PhysicalPosition::new(-214, 170);
        let (position, size) = drawing_surface_geometry(from, to, Some(anchor));
        assert_eq!(position, anchor);
        assert_eq!(size, PhysicalSize::new(516, 631));

        // Once hidden, the same window can use a compact origin on its next opening.
        let (position, size) = drawing_surface_geometry(from, to, None);
        assert_eq!(position, PhysicalPosition::new(-123, 270));
        assert_eq!(size, PhysicalSize::new(425, 531));

        // An incoming path above/left of the existing canvas still remains drawable.
        let (position, size) = drawing_surface_geometry(
            Point {
                x: -320.5,
                y: 150.25,
            },
            to,
            Some(anchor),
        );
        assert_eq!(position, PhysicalPosition::new(-321, 150));
        assert_eq!(size, PhysicalSize::new(623, 651));
    }
}
