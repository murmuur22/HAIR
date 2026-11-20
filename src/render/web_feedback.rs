//! Bounded screen-space web-attempt presentation. Never owns a room, URL binding,
//! source, timer gate, input lock, texture or navigation transaction.
use crate::{assets::Vertex, text::Painter};
use std::fmt::Write;
#[cfg(test)]
#[path = "web_feedback_tests.rs"]
mod tests;

pub const MAX_VERTICES: usize = 100_002; // triangle-aligned, independent of content
pub const MAX_DESTINATION_DISPLAY: usize = 16_384;
pub const MAX_REASON_DISPLAY: usize = 4096;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Hidden,
    Pending,
    FailureRed,
    FailureSettled,
    Cancelled,
}
#[derive(Default)]
pub struct Snapshot {
    pub mode: Mode,
    /// Escaped presentation only. Raw identity stays in App Job/FeedbackRecord.
    pub destination: String,
    pub reason: String,
    pub seconds: f32,
    pub cached: bool,
    pub retiring: bool,
    pub details: bool,
    pub scroll: usize,
    pub static_motion: bool,
}
impl Snapshot {
    // One finite adapter call, no settings framework or renderer dependency on App.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        mode: Mode,
        destination: &str,
        reason: Option<&str>,
        seconds: f32,
        cached: bool,
        retiring: bool,
        details: bool,
        scroll: usize,
        static_motion: bool,
    ) -> Self {
        Self {
            mode,
            destination: escaped(destination, 2048, MAX_DESTINATION_DISPLAY),
            reason: escaped(reason.unwrap_or(""), 1024, MAX_REASON_DISPLAY),
            seconds: if seconds.is_finite() {
                seconds.rem_euclid(120.0)
            } else {
                0.0
            },
            cached,
            retiring,
            details,
            scroll,
            static_motion,
        }
    }
    pub fn status(&self) -> &'static str {
        match self.mode {
            Mode::Hidden => "",
            Mode::Pending => "Loading…",
            Mode::FailureRed | Mode::FailureSettled => "Load failed",
            Mode::Cancelled => "Cancelled",
        }
    }
    pub fn controls(&self) -> &'static str {
        match self.mode {
            Mode::Pending => "R cancel | F8 details",
            Mode::Cancelled if self.retiring => "Retiring | F8 details",
            _ => "L address | F8 details",
        }
    }
    fn layout(size: [u32; 2], details: bool) -> (f32, f32, usize, usize) {
        // Reserve the existing left outline, top header, crosshair and bottom HUD.
        let left = (size[0] as f32 * 0.32).min(260.0) + 36.0;
        let width = (size[0] as f32 - left - 12.0).min(480.0);
        let height = if details {
            (size[1] as f32 * 0.5 - 92.0).clamp(106.0, 200.0)
        } else {
            106.0
        };
        let columns = ((width - 20.0) / 12.0).max(1.0) as usize;
        let rows = (((height - 78.0) / 20.0).max(1.0) as usize).min(6);
        (width, height, columns, rows)
    }
    pub fn visible_rows(&self, size: [u32; 2]) -> Vec<String> {
        let (_, _, columns, rows) = Self::layout(size, true);
        let lines = [
            "Destination (exact; escaped):",
            self.destination.as_str(),
            "Reason (escaped):",
            self.reason.as_str(),
            if self.cached {
                "Cached source; no fetch"
            } else {
                "Last good place retained"
            },
        ];
        let total: usize = lines.iter().map(|s| s.len().max(1).div_ceil(columns)).sum();
        let mut skip = self.scroll.min(total.saturating_sub(rows));
        let mut output = Vec::with_capacity(rows);
        for line in lines {
            let count = line.len().max(1).div_ceil(columns);
            if skip >= count {
                skip -= count;
                continue;
            }
            if line.is_empty() {
                output.push(String::new());
            } else {
                for chunk in line
                    .as_bytes()
                    .chunks(columns)
                    .skip(skip)
                    .take(rows - output.len())
                {
                    // escaped() guarantees ASCII; no replacement or identity normalization.
                    output.push(String::from_utf8_lossy(chunk).into_owned());
                }
            }
            skip = 0;
            if output.len() == rows {
                break;
            }
        }
        output
    }
    /// Reuses the renderer-owned CPU vector. Fixed text rows and spiral segments;
    /// no scene regeneration, source parsing or image upload in this path.
    pub fn paint(&self, vertices: &mut Vec<Vertex>, size: [u32; 2], context_owned: bool) {
        vertices.clear();
        if self.mode == Mode::Hidden
            || size[0] < 640
            || size[1] < 480
            || size.iter().any(|s| *s > 4096)
        {
            return;
        }
        let mut p = Painter {
            vertices: std::mem::take(vertices),
            width: size[0] as f32,
            height: size[1] as f32,
        };
        let red = matches!(self.mode, Mode::FailureRed | Mode::FailureSettled);
        let color = if red {
            [1.0, 0.38, 0.32]
        } else if self.mode == Mode::Cancelled {
            [0.75; 3]
        } else {
            [0.25, 0.95, 0.72]
        };
        if context_owned {
            // The reader/address owns its body/footer. Use only the header gap.
            p.rect(0.0, 52.0, p.width, 16.0, [0.025, 0.025, 0.025]);
            let line = format!(
                "{} | {} | Esc then F8 details{}",
                self.status(),
                if self.mode == Mode::Pending {
                    "Esc then R cancel"
                } else {
                    "last good retained"
                },
                if self.reason.is_empty() {
                    String::new()
                } else {
                    format!(" | {}", clipped(&self.reason, 64))
                }
            );
            p.literal_text(
                12.0,
                56.0,
                &clipped(&line, (size[0] as usize - 48) / 6),
                1.0,
                color,
            );
            self.swirl(&mut p, [size[0] as f32 - 12.0, 60.0], 5.0);
        } else {
            let (width, height, columns, _) = Self::layout(size, self.details);
            let x = size[0] as f32 - width - 12.0;
            let top = 76.0;
            p.rect(x, top, width, height, [0.012, 0.025, 0.022]);
            p.rect(x, top, 3.0, height, color);
            p.literal_text(x + 10.0, top + 8.0, self.status(), 2.0, color);
            p.literal_text(x + 10.0, top + 30.0, self.controls(), 1.0, [0.88; 3]);
            self.swirl(&mut p, [x + width - 28.0, top + 26.0], 20.0);
            if self.details {
                for (i, line) in self.visible_rows(size).iter().enumerate() {
                    p.literal_text(
                        x + 10.0,
                        top + 54.0 + i as f32 * 20.0,
                        line,
                        2.0,
                        [0.9, 0.96, 0.91],
                    );
                }
                p.literal_text(
                    x + 10.0,
                    top + height - 14.0,
                    "Up/Down scroll | F8 close",
                    1.0,
                    [0.88; 3],
                );
            } else {
                p.literal_text(
                    x + 10.0,
                    top + 54.0,
                    &clipped(&self.destination, columns),
                    2.0,
                    [0.9, 0.96, 0.91],
                );
                let summary = if !self.reason.is_empty() {
                    self.reason.as_str()
                } else if self.cached {
                    "Cached source; no fetch"
                } else {
                    "Last good place retained"
                };
                p.literal_text(x + 10.0, top + 76.0, &clipped(summary, columns), 2.0, color);
            }
        }
        debug_assert!(p.vertices.len() <= MAX_VERTICES);
        // Defense in depth if a future glyph/layout change breaks the count proof.
        p.vertices.truncate(MAX_VERTICES);
        *vertices = p.vertices;
    }
    fn swirl(&self, p: &mut Painter, center: [f32; 2], radius: f32) {
        if !matches!(self.mode, Mode::Pending | Mode::FailureRed) {
            return;
        }
        // Three fixed spiral ribbons, 32 segments each, no pulse/brightness cycle.
        // Period 12 seconds; f32 time remains finite and bounded at the adapter.
        let seconds = if self.seconds.is_finite() {
            self.seconds.rem_euclid(120.0)
        } else {
            0.0
        };
        let phase = if self.static_motion {
            0.0
        } else {
            seconds * std::f32::consts::TAU / 12.0
        };
        for arm in 0..3 {
            for segment in 0..32 {
                let point = |step: usize, edge: f32| {
                    let t = step as f32 / 32.0;
                    let angle = phase + arm as f32 * std::f32::consts::TAU / 3.0 + t * 5.0;
                    let r = radius * (0.18 + 0.72 * t) + edge * radius * 0.035;
                    [center[0] + angle.cos() * r, center[1] + angle.sin() * r]
                };
                let a = point(segment, -1.0);
                let b = point(segment, 1.0);
                let c = point(segment + 1, 1.0);
                let d = point(segment + 1, -1.0);
                let color = if self.mode == Mode::FailureRed {
                    [0.95, 0.16 + arm as f32 * 0.06, 0.13]
                } else {
                    [
                        0.08 + arm as f32 * 0.08,
                        0.72 + arm as f32 * 0.08,
                        0.42 + arm as f32 * 0.16,
                    ]
                };
                for [x, y] in [a, b, c, a, c, d] {
                    p.vertices.push(Vertex {
                        position: [2.0 * x / p.width - 1.0, 1.0 - 2.0 * y / p.height, 0.0],
                        color,
                        uv: [0.0; 2],
                        texture: 0,
                    });
                }
            }
        }
    }
}
/// Bounded escaped display of the inherited status/error channel. Its raw record
/// remains untouched; a shortened view is explicitly marked rather than hidden.
pub fn display_status(raw: &str) -> String {
    escaped(raw, 1024, MAX_REASON_DISPLAY)
}
fn clipped(text: &str, columns: usize) -> String {
    let mut out: String = text.chars().take(columns).collect();
    if text.len() > out.len() && columns > 0 {
        out.pop();
        out.push('~');
    }
    out
}
/// Read at most max_scalars + 1, emit complete visible escapes and an explicit
/// excerpt marker. Never scan the remainder, interpret controls, or alter raw IDs.
fn escaped(text: &str, max_scalars: usize, max_bytes: usize) -> String {
    let mut out = String::with_capacity(text.len().min(max_bytes));
    let mut chars = text.chars();
    let mut truncated = false;
    for _ in 0..max_scalars {
        let Some(c) = chars.next() else {
            return out;
        };
        let needed = if c.is_ascii_graphic() || c == ' ' {
            1
        } else if c.is_ascii() {
            4
        } else {
            10
        };
        if out.len() + needed + "[excerpt]".len() > max_bytes {
            truncated = true;
            break;
        }
        if c.is_ascii_graphic() || c == ' ' {
            out.push(c);
        } else if c.is_ascii() {
            let _ = write!(out, "\\x{:02x}", c as u32);
        } else {
            let _ = write!(out, "\\u{{{:x}}}", c as u32);
        }
    }
    if truncated || chars.next().is_some() {
        out.push_str("[excerpt]");
    }
    out
}
