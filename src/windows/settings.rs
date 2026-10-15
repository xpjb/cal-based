use based::{
    ElementState, Event, EventFlow, HostEvent, InputEvent, Key, MouseButton, NamedKey, TouchPhase,
    Widget,
    backend::winit::raw::window::CursorIcon,
    glam::{Vec2, Vec4},
};
use cal_based::model::{CalendarSettings, CalendarTxn, DayColors, RgbColor};

use super::{
    AppContext,
    paint::{HorizontalAlign, Rect, VerticalAlign},
    ui::{
        ACCENT, Button, ButtonRelease, GRID, MUTED, NEUTRAL_BUTTON, PRIMARY_BUTTON, StateColors,
        SurfaceState, SurfaceStyle, TEXT, paint_surface,
    },
};

const MOUSE_POINTER: u64 = u64::MAX;
const PANEL_BG: Vec4 = Vec4::new(0.075, 0.088, 0.108, 1.0);
const ROW_STYLE: SurfaceStyle = SurfaceStyle {
    fill: StateColors::new(
        Vec4::new(0.045, 0.054, 0.068, 1.0),
        Vec4::new(0.065, 0.082, 0.105, 1.0),
        Vec4::new(0.035, 0.045, 0.060, 1.0),
        Vec4::new(0.075, 0.105, 0.145, 1.0),
    ),
    border: StateColors::new(
        GRID,
        Vec4::new(0.28, 0.39, 0.52, 1.0),
        Vec4::new(0.34, 0.52, 0.72, 1.0),
        ACCENT,
    ),
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum DayColorSlot {
    PastWeekday,
    TodayWeekday,
    UpcomingWeekday,
    PastWeekend,
    TodayWeekend,
    UpcomingWeekend,
}

impl DayColorSlot {
    const ALL: [Self; 6] = [
        Self::PastWeekday,
        Self::TodayWeekday,
        Self::UpcomingWeekday,
        Self::PastWeekend,
        Self::TodayWeekend,
        Self::UpcomingWeekend,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::PastWeekday => "Past weekday",
            Self::TodayWeekday => "Today weekday",
            Self::UpcomingWeekday => "Upcoming weekday",
            Self::PastWeekend => "Past weekend",
            Self::TodayWeekend => "Today weekend",
            Self::UpcomingWeekend => "Upcoming weekend",
        }
    }

    fn color(self, colors: DayColors) -> RgbColor {
        match self {
            Self::PastWeekday => colors.past_weekday,
            Self::TodayWeekday => colors.today_weekday,
            Self::UpcomingWeekday => colors.upcoming_weekday,
            Self::PastWeekend => colors.past_weekend,
            Self::TodayWeekend => colors.today_weekend,
            Self::UpcomingWeekend => colors.upcoming_weekend,
        }
    }

    fn set_color(self, colors: &mut DayColors, color: RgbColor) {
        match self {
            Self::PastWeekday => colors.past_weekday = color,
            Self::TodayWeekday => colors.today_weekday = color,
            Self::UpcomingWeekday => colors.upcoming_weekday = color,
            Self::PastWeekend => colors.past_weekend = color,
            Self::TodayWeekend => colors.today_weekend = color,
            Self::UpcomingWeekend => colors.upcoming_weekend = color,
        }
    }
}

#[derive(Clone, Copy)]
struct SettingsLayout {
    panel: Rect,
    rows: [Rect; 6],
    saturation_value: Rect,
    hue: Rect,
    reset: Rect,
    done: Rect,
}

#[derive(Clone, Copy)]
enum PickerTarget {
    SaturationValue,
    Hue,
}

pub struct SettingsWidget {
    draft: Option<CalendarSettings>,
    selected: DayColorSlot,
    hue: f32,
    saturation: f32,
    value: f32,
    layout: Option<SettingsLayout>,
    cursor: Vec2,
    picker: Option<(u64, PickerTarget)>,
    rows: [Button; 6],
    reset_button: Button,
    done_button: Button,
    error: Option<String>,
}

impl SettingsWidget {
    pub fn new() -> Self {
        Self {
            draft: None,
            selected: DayColorSlot::PastWeekday,
            hue: 0.0,
            saturation: 0.0,
            value: 0.0,
            layout: None,
            cursor: Vec2::ZERO,
            picker: None,
            rows: std::array::from_fn(|_| Button::default()),
            reset_button: Button::default(),
            done_button: Button::default(),
            error: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.draft.is_some()
    }

    pub fn open(&mut self, context: &mut AppContext<'_>) {
        let settings = context.model.calendar().settings().clone();
        let color = self.selected.color(settings.day_colors);
        (self.hue, self.saturation, self.value) = rgb_to_hsv(color);
        self.draft = Some(settings);
        self.error = None;
        context.request_redraw();
    }

    fn close(&mut self, context: &mut AppContext<'_>) {
        let Some(settings) = self.draft.clone() else {
            return;
        };
        match context
            .model
            .transact(vec![CalendarTxn::SetSettings { settings }])
        {
            Ok(()) => {
                self.draft = None;
                self.layout = None;
                self.picker = None;
                self.error = None;
                for row in &mut self.rows {
                    row.cancel();
                }
                self.reset_button.cancel();
                self.done_button.cancel();
                context.set_cursor(CursorIcon::Default);
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        context.request_redraw();
    }

    fn update_picker(&mut self, target: PickerTarget, point: Vec2) {
        let Some(layout) = self.layout else {
            return;
        };
        match target {
            PickerTarget::SaturationValue => {
                self.saturation = ((point.x - layout.saturation_value.position.x)
                    / layout.saturation_value.size.x)
                    .clamp(0.0, 1.0);
                self.value = (1.0
                    - (point.y - layout.saturation_value.position.y)
                        / layout.saturation_value.size.y)
                    .clamp(0.0, 1.0);
            }
            PickerTarget::Hue => {
                self.hue = ((point.x - layout.hue.position.x) / layout.hue.size.x)
                    .clamp(0.0, 1.0);
            }
        }
        if let Some(settings) = &mut self.draft {
            self.selected.set_color(
                &mut settings.day_colors,
                hsv_to_rgb(self.hue, self.saturation, self.value),
            );
        }
    }

    fn pointer_move(
        &mut self,
        context: &mut AppContext<'_>,
        pointer: u64,
        point: Vec2,
        hover_capable: bool,
    ) {
        self.cursor = point;
        let mut changed = false;
        for row in &mut self.rows {
            changed |= row.pointer_moved(pointer, point, hover_capable);
        }
        changed |= self
            .reset_button
            .pointer_moved(pointer, point, hover_capable);
        changed |= self
            .done_button
            .pointer_moved(pointer, point, hover_capable);
        if let Some((active, target)) = self.picker
            && active == pointer
        {
            self.update_picker(target, point);
            changed = true;
        }
        if hover_capable {
            let cursor = self.layout.map_or(CursorIcon::Default, |layout| {
                if layout.saturation_value.contains(point) {
                    CursorIcon::Crosshair
                } else if layout.hue.contains(point) {
                    CursorIcon::EwResize
                } else if layout.done.contains(point)
                    || layout.reset.contains(point)
                    || layout.rows.iter().any(|rect| rect.contains(point))
                {
                    CursorIcon::Pointer
                } else {
                    CursorIcon::Default
                }
            });
            context.set_cursor(cursor);
        }
        if changed {
            context.request_redraw();
        }
    }

    fn pointer_down(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        self.cursor = point;
        let Some(layout) = self.layout else {
            return;
        };
        if !layout.panel.contains(point) {
            self.close(context);
            return;
        }
        if self.done_button.press(pointer, point)
            || self.reset_button.press(pointer, point)
            || self.rows.iter_mut().any(|row| row.press(pointer, point))
        {
            context.request_redraw();
        } else if layout.saturation_value.contains(point) {
            self.picker = Some((pointer, PickerTarget::SaturationValue));
            self.update_picker(PickerTarget::SaturationValue, point);
            context.request_redraw();
        } else if layout.hue.contains(point) {
            self.picker = Some((pointer, PickerTarget::Hue));
            self.update_picker(PickerTarget::Hue, point);
            context.request_redraw();
        }
    }

    fn pointer_up(&mut self, context: &mut AppContext<'_>, pointer: u64, point: Vec2) {
        self.cursor = point;
        self.picker = None;
        let done = self.done_button.release(pointer, point);
        let reset = self.reset_button.release(pointer, point);
        let releases = std::array::from_fn::<_, 6, _>(|index| {
            self.rows[index].release(pointer, point)
        });
        if done == ButtonRelease::Activated {
            self.close(context);
            return;
        }
        if reset == ButtonRelease::Activated {
            if let Some(settings) = &mut self.draft {
                *settings = CalendarSettings::default();
                let color = self.selected.color(settings.day_colors);
                (self.hue, self.saturation, self.value) = rgb_to_hsv(color);
            }
        } else if let Some(index) = releases
            .iter()
            .position(|release| *release == ButtonRelease::Activated)
        {
            self.selected = DayColorSlot::ALL[index];
            if let Some(settings) = &self.draft {
                let color = self.selected.color(settings.day_colors);
                (self.hue, self.saturation, self.value) = rgb_to_hsv(color);
            }
        }
        context.request_redraw();
    }
}

impl Widget<AppContext<'_>, Event> for SettingsWidget {
    fn handle_event(&mut self, context: &mut AppContext<'_>, event: &Event) -> EventFlow {
        let Some(settings) = self.draft.clone() else {
            return EventFlow::NotConsumed;
        };

        if matches!(event, Event::Host(HostEvent::RedrawRequested)) {
            if let Some(point) = context.input.pointer() {
                self.cursor = point;
            }
            let viewport = context.viewport().as_vec2();
            let scale = context.scale_factor();
            let panel_width = (viewport.x - 32.0 * scale).min(860.0 * scale);
            let panel_height = (viewport.y - 32.0 * scale).min(640.0 * scale);
            let panel = Rect::new(
                (viewport.x - panel_width) * 0.5,
                (viewport.y - panel_height) * 0.5,
                panel_width,
                panel_height,
            );
            let padding = 24.0 * scale;
            let content_top = panel.position.y + 78.0 * scale;
            let left_width = (panel.size.x * 0.40).clamp(250.0 * scale, 340.0 * scale);
            let row_height = 45.0 * scale;
            let row_gap = 7.0 * scale;
            let rows = std::array::from_fn(|index| {
                Rect::new(
                    panel.position.x + padding,
                    content_top + index as f32 * (row_height + row_gap),
                    left_width,
                    row_height,
                )
            });
            let picker_x = panel.position.x + padding + left_width + 24.0 * scale;
            let picker_width = (panel.right() - padding - picker_x).max(100.0 * scale);
            let picker_size = picker_width
                .min(panel.size.y - 198.0 * scale)
                .max(100.0 * scale);
            let saturation_value = Rect::new(picker_x, content_top, picker_size, picker_size);
            let hue = Rect::new(
                picker_x,
                saturation_value.bottom() + 16.0 * scale,
                picker_size,
                22.0 * scale,
            );
            let done = Rect::new(
                panel.right() - padding - 100.0 * scale,
                panel.bottom() - 58.0 * scale,
                100.0 * scale,
                38.0 * scale,
            );
            let reset = Rect::new(
                done.position.x - 112.0 * scale,
                done.position.y,
                100.0 * scale,
                38.0 * scale,
            );
            let layout = SettingsLayout {
                panel,
                rows,
                saturation_value,
                hue,
                reset,
                done,
            };
            self.layout = Some(layout);
            for (button, rect) in self.rows.iter_mut().zip(rows) {
                button.set_rect(rect);
                button.set_hover(self.cursor);
            }
            self.reset_button.set_rect(reset);
            self.done_button.set_rect(done);
            self.reset_button.set_hover(self.cursor);
            self.done_button.set_hover(self.cursor);

            context.quad(
                Rect::new(0.0, 0.0, viewport.x, viewport.y),
                Vec4::new(0.0, 0.0, 0.0, 0.74),
            );
            context.surface(
                panel,
                12.0 * scale,
                scale,
                PANEL_BG,
                Vec4::new(0.30, 0.35, 0.42, 1.0),
            );
            let _ = context.text(
                "Day colors",
                Rect::new(
                    panel.position.x + padding,
                    panel.position.y + 18.0 * scale,
                    panel.size.x - padding * 2.0,
                    38.0 * scale,
                ),
                22.0 * scale,
                TEXT,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );

            for (index, slot) in DayColorSlot::ALL.into_iter().enumerate() {
                let state = if slot == self.selected {
                    SurfaceState::Dragging
                } else {
                    self.rows[index].state()
                };
                paint_surface(
                    context,
                    rows[index],
                    7.0 * scale,
                    scale,
                    state,
                    ROW_STYLE,
                );
                let _ = context.text(
                    slot.label(),
                    rows[index].inset(12.0 * scale),
                    13.5 * scale,
                    TEXT,
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
                let chip = Rect::new(
                    rows[index].right() - 50.0 * scale,
                    rows[index].position.y + 8.0 * scale,
                    38.0 * scale,
                    rows[index].size.y - 16.0 * scale,
                );
                context.surface(
                    chip,
                    6.0 * scale,
                    if slot == self.selected {
                        2.0 * scale
                    } else {
                        scale
                    },
                    Vec4::from_array(slot.color(settings.day_colors).rgba()),
                    if slot == self.selected { ACCENT } else { GRID },
                );
            }

            let hue_color = Vec4::from_array(hsv_to_rgb(self.hue, 1.0, 1.0).rgba());
            context.gradient(
                saturation_value,
                [Vec4::ONE, hue_color, hue_color, Vec4::ONE],
            );
            context.gradient(
                saturation_value,
                [
                    Vec4::ZERO,
                    Vec4::ZERO,
                    Vec4::new(0.0, 0.0, 0.0, 1.0),
                    Vec4::new(0.0, 0.0, 0.0, 1.0),
                ],
            );
            context.surface(
                saturation_value,
                0.0,
                scale,
                Vec4::ZERO,
                Vec4::new(0.38, 0.43, 0.51, 1.0),
            );
            let hue_stops = [
                RgbColor::new(255, 0, 0),
                RgbColor::new(255, 255, 0),
                RgbColor::new(0, 255, 0),
                RgbColor::new(0, 255, 255),
                RgbColor::new(0, 0, 255),
                RgbColor::new(255, 0, 255),
                RgbColor::new(255, 0, 0),
            ];
            for index in 0..6 {
                let segment = Rect::new(
                    hue.position.x + hue.size.x * index as f32 / 6.0,
                    hue.position.y,
                    hue.size.x / 6.0,
                    hue.size.y,
                );
                let left = Vec4::from_array(hue_stops[index].rgba());
                let right = Vec4::from_array(hue_stops[index + 1].rgba());
                context.gradient(segment, [left, right, right, left]);
            }
            context.surface(
                hue,
                0.0,
                scale,
                Vec4::ZERO,
                Vec4::new(0.38, 0.43, 0.51, 1.0),
            );
            let marker = Vec2::new(
                saturation_value.position.x + self.saturation * saturation_value.size.x,
                saturation_value.position.y + (1.0 - self.value) * saturation_value.size.y,
            );
            context.surface(
                Rect::new(
                    marker.x - 6.0 * scale,
                    marker.y - 6.0 * scale,
                    12.0 * scale,
                    12.0 * scale,
                ),
                6.0 * scale,
                2.0 * scale,
                Vec4::ZERO,
                Vec4::ONE,
            );
            let hue_x = hue.position.x + self.hue * hue.size.x;
            context.surface(
                Rect::new(
                    hue_x - 2.0 * scale,
                    hue.position.y - 3.0 * scale,
                    4.0 * scale,
                    hue.size.y + 6.0 * scale,
                ),
                2.0 * scale,
                scale,
                Vec4::new(0.03, 0.04, 0.05, 1.0),
                Vec4::ONE,
            );
            let selected_color = self.selected.color(settings.day_colors);
            let value = format!(
                "{}   #{:02X}{:02X}{:02X}",
                self.selected.label(),
                selected_color.r,
                selected_color.g,
                selected_color.b,
            );
            let _ = context.text(
                &value,
                Rect::new(
                    picker_x,
                    hue.bottom() + 14.0 * scale,
                    picker_size,
                    26.0 * scale,
                ),
                13.0 * scale,
                MUTED,
                HorizontalAlign::Left,
                VerticalAlign::Center,
                false,
            );
            if let Some(error) = &self.error {
                let _ = context.text(
                    error,
                    Rect::new(
                        panel.position.x + padding,
                        done.position.y,
                        reset.position.x - panel.position.x - padding * 2.0,
                        done.size.y,
                    ),
                    12.5 * scale,
                    Vec4::new(1.0, 0.55, 0.58, 1.0),
                    HorizontalAlign::Left,
                    VerticalAlign::Center,
                    false,
                );
            }
            self.reset_button
                .paint(context, "Reset", 13.0, scale, NEUTRAL_BUTTON);
            self.done_button
                .paint(context, "Done", 13.0, scale, PRIMARY_BUTTON);
            return EventFlow::Consumed;
        }

        match event {
            Event::Input(InputEvent::CursorMoved { position, .. }) => {
                self.pointer_move(context, MOUSE_POINTER, position.as_vec2(), true);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            }) => {
                let point = context.input.pointer().unwrap_or(self.cursor);
                self.pointer_down(context, MOUSE_POINTER, point);
            }
            Event::Input(InputEvent::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            }) => {
                let point = context.input.pointer().unwrap_or(self.cursor);
                self.pointer_up(context, MOUSE_POINTER, point);
            }
            Event::Input(InputEvent::Touch(touch)) => {
                let point = touch.position.as_vec2();
                match touch.phase {
                    TouchPhase::Started => self.pointer_down(context, touch.id, point),
                    TouchPhase::Moved => self.pointer_move(context, touch.id, point, false),
                    TouchPhase::Ended => self.pointer_up(context, touch.id, point),
                    TouchPhase::Cancelled => {
                        self.picker = None;
                        for row in &mut self.rows {
                            row.cancel();
                        }
                        self.reset_button.cancel();
                        self.done_button.cancel();
                        context.request_redraw();
                    }
                }
            }
            Event::Input(InputEvent::Keyboard(key))
                if key.state == ElementState::Pressed
                    && matches!(key.logical_key, Key::Named(NamedKey::Escape)) =>
            {
                self.close(context);
            }
            Event::Host(HostEvent::Focused { focused: false }) => {
                self.picker = None;
                for row in &mut self.rows {
                    row.cancel();
                }
                self.reset_button.cancel();
                self.done_button.cancel();
                context.set_cursor(CursorIcon::Default);
                context.request_redraw();
            }
            _ => {}
        }
        EventFlow::Consumed
    }
}

fn rgb_to_hsv(color: RgbColor) -> (f32, f32, f32) {
    let [red, green, blue, _] = color.rgba();
    let maximum = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let delta = maximum - minimum;
    let hue = if delta == 0.0 {
        0.0
    } else if maximum == red {
        ((green - blue) / delta).rem_euclid(6.0) / 6.0
    } else if maximum == green {
        ((blue - red) / delta + 2.0) / 6.0
    } else {
        ((red - green) / delta + 4.0) / 6.0
    };
    let saturation = if maximum == 0.0 { 0.0 } else { delta / maximum };
    (hue, saturation, maximum)
}

fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> RgbColor {
    let hue = hue.rem_euclid(1.0) * 6.0;
    let chroma = value * saturation;
    let x = chroma * (1.0 - (hue.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue as u32 {
        0 => (chroma, x, 0.0),
        1 => (x, chroma, 0.0),
        2 => (0.0, chroma, x),
        3 => (0.0, x, chroma),
        4 => (x, 0.0, chroma),
        _ => (chroma, 0.0, x),
    };
    let offset = value - chroma;
    let encode = |channel: f32| {
        let value = if channel <= 0.003_130_8 {
            channel * 12.92
        } else {
            1.055 * channel.powf(1.0 / 2.4) - 0.055
        };
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    };
    RgbColor::new(
        encode(red + offset),
        encode(green + offset),
        encode(blue + offset),
    )
}
