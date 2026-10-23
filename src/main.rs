mod metrics;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};
use gilrs::{EventType, GamepadId, Gilrs};
use metrics::{IntervalSeries, IntervalSummary};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

const BACKGROUND: Color32 = Color32::from_rgb(245, 247, 243);
const SIDEBAR: Color32 = Color32::from_rgb(231, 237, 229);
const INK: Color32 = Color32::from_rgb(31, 43, 38);
const MUTED: Color32 = Color32::from_rgb(106, 119, 111);
const GREEN: Color32 = Color32::from_rgb(35, 128, 92);
const CORAL: Color32 = Color32::from_rgb(217, 112, 86);
const LINE: Color32 = Color32::from_rgb(211, 220, 212);
const MAX_RECENT_EVENTS: usize = 80;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1_120.0, 760.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Gamepad Latency Tester",
        options,
        Box::new(|creation_context| {
            configure_theme(&creation_context.egui_ctx);
            Ok(Box::new(GamepadApp::new()))
        }),
    )
}

fn configure_theme(context: &egui::Context) {
    let mut visuals = egui::Visuals::light();
    visuals.panel_fill = BACKGROUND;
    visuals.window_fill = BACKGROUND;
    visuals.extreme_bg_color = Color32::from_rgb(255, 255, 252);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(238, 242, 236);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(222, 233, 221);
    visuals.widgets.active.bg_fill = Color32::from_rgb(205, 225, 211);
    visuals.selection.bg_fill = GREEN;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    context.set_visuals(visuals);
}

struct PadState {
    name: String,
    buttons: HashMap<String, f32>,
    axes: HashMap<String, f32>,
    intervals: IntervalSeries,
    last_input_at: Option<Instant>,
    event_count: u64,
    last_event: String,
}

impl PadState {
    fn new(name: String) -> Self {
        Self {
            name,
            buttons: HashMap::new(),
            axes: HashMap::new(),
            intervals: IntervalSeries::default(),
            last_input_at: None,
            event_count: 0,
            last_event: "Waiting for input".to_owned(),
        }
    }

    fn reset(&mut self) {
        self.buttons.clear();
        self.axes.clear();
        self.intervals.clear();
        self.last_input_at = None;
        self.event_count = 0;
        self.last_event = "Waiting for input".to_owned();
    }
}

struct RecentEvent {
    received_at: Instant,
    device_name: String,
    description: String,
    interval_ms: Option<f64>,
}

struct GamepadApp {
    gilrs: Option<Gilrs>,
    initialization_error: Option<String>,
    devices: HashMap<GamepadId, PadState>,
    device_order: Vec<GamepadId>,
    selected_device: Option<GamepadId>,
    recent_events: VecDeque<RecentEvent>,
    session_started: Instant,
}

impl GamepadApp {
    fn new() -> Self {
        let mut app = Self {
            gilrs: None,
            initialization_error: None,
            devices: HashMap::new(),
            device_order: Vec::new(),
            selected_device: None,
            recent_events: VecDeque::new(),
            session_started: Instant::now(),
        };
        app.initialize_input();
        app
    }

    fn initialize_input(&mut self) {
        match Gilrs::new() {
            Ok(gilrs) => {
                self.gilrs = Some(gilrs);
                self.initialization_error = None;
                self.refresh_devices();
            }
            Err(error) => {
                self.gilrs = None;
                self.initialization_error = Some(error.to_string());
            }
        }
    }

    fn poll_events(&mut self) {
        loop {
            let next_event = match self.gilrs.as_mut() {
                Some(gilrs) => gilrs.next_event(),
                None => return,
            };

            match next_event {
                Some(event) => self.handle_event(event.id, event.event),
                None => break,
            }
        }

        self.refresh_devices();
    }

    fn handle_event(&mut self, id: GamepadId, event: EventType) {
        match event {
            EventType::Connected => {
                self.refresh_devices();
                self.push_recent_event(id, "Controller connected".to_owned(), None);
            }
            EventType::Disconnected => {
                let name = self
                    .devices
                    .get(&id)
                    .map(|device| device.name.clone())
                    .unwrap_or_else(|| format!("Controller {id}"));
                self.devices.remove(&id);
                self.device_order.retain(|device_id| *device_id != id);
                if self.selected_device == Some(id) {
                    self.selected_device = self.device_order.first().copied();
                }
                self.push_recent_event_named(name, "Controller disconnected".to_owned(), None);
            }
            EventType::ButtonPressed(button, _) => {
                self.record_input(id, true, format!("{button:?}"), 1.0, "pressed");
            }
            EventType::ButtonReleased(button, _) => {
                self.record_input(id, true, format!("{button:?}"), 0.0, "released");
            }
            EventType::ButtonChanged(button, value, _) => {
                self.record_input(id, true, format!("{button:?}"), value, "value changed");
            }
            EventType::AxisChanged(axis, value, _) => {
                self.record_input(id, false, format!("{axis:?}"), value, "axis changed");
            }
            _ => {}
        }
    }

    fn record_input(
        &mut self,
        id: GamepadId,
        is_button: bool,
        control: String,
        value: f32,
        action: &str,
    ) {
        let received_at = Instant::now();
        let device_name = self
            .devices
            .get(&id)
            .map(|device| device.name.clone())
            .unwrap_or_else(|| format!("Controller {id}"));
        let value_suffix = if action == "pressed" || action == "released" {
            String::new()
        } else {
            format!(" ({value:+.2})")
        };
        let description = format!("{control} {action}{value_suffix}");

        let device = self
            .devices
            .entry(id)
            .or_insert_with(|| PadState::new(device_name.clone()));
        device.name = device_name.clone();

        let interval = device.last_input_at.replace(received_at).map(|previous| {
            let interval = received_at.saturating_duration_since(previous);
            device.intervals.push(interval);
            interval.as_secs_f64() * 1_000.0
        });

        let state = if is_button {
            &mut device.buttons
        } else {
            &mut device.axes
        };
        state.insert(control, value);
        device.event_count += 1;
        device.last_event = description.clone();

        self.push_recent_event_named(device_name, description, interval);
    }

    fn push_recent_event(&mut self, id: GamepadId, description: String, interval_ms: Option<f64>) {
        let name = self
            .devices
            .get(&id)
            .map(|device| device.name.clone())
            .unwrap_or_else(|| format!("Controller {id}"));
        self.push_recent_event_named(name, description, interval_ms);
    }

    fn push_recent_event_named(
        &mut self,
        device_name: String,
        description: String,
        interval_ms: Option<f64>,
    ) {
        self.recent_events.push_front(RecentEvent {
            received_at: Instant::now(),
            device_name,
            description,
            interval_ms,
        });
        self.recent_events.truncate(MAX_RECENT_EVENTS);
    }

    fn refresh_devices(&mut self) {
        let connected: Vec<(GamepadId, String)> = match self.gilrs.as_ref() {
            Some(gilrs) => gilrs
                .gamepads()
                .map(|(id, gamepad)| (id, gamepad.name().to_owned()))
                .collect(),
            None => Vec::new(),
        };

        self.device_order = connected.iter().map(|(id, _)| *id).collect();
        self.devices.retain(|id, _| {
            self.device_order
                .iter()
                .any(|connected_id| connected_id == id)
        });

        for (id, name) in connected {
            self.devices
                .entry(id)
                .and_modify(|device| device.name.clone_from(&name))
                .or_insert_with(|| PadState::new(name));
        }

        if self
            .selected_device
            .is_none_or(|id| !self.devices.contains_key(&id))
        {
            self.selected_device = self.device_order.first().copied();
        }
    }

    fn clear_session(&mut self) {
        self.recent_events.clear();
        self.session_started = Instant::now();
        for device in self.devices.values_mut() {
            device.reset();
        }
    }

    fn selected_state(&self) -> Option<&PadState> {
        self.selected_device.and_then(|id| self.devices.get(&id))
    }

    fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("device_sidebar")
            .resizable(false)
            .default_size(258.0)
            .frame(egui::Frame::new().fill(SIDEBAR).inner_margin(20.0))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("INPUT / LAB").strong().color(GREEN));
                    ui.add_space(12.0);
                    ui.heading(egui::RichText::new("Controller\nbench").size(25.0).color(INK));
                });

                ui.add_space(28.0);
                ui.horizontal(|ui| {
                    status_dot(ui, self.gilrs.is_some(), 10.0);
                    ui.label(
                        egui::RichText::new(if self.gilrs.is_some() {
                            "INPUT SERVICE READY"
                        } else {
                            "INPUT SERVICE OFFLINE"
                        })
                        .small()
                        .strong()
                        .color(MUTED),
                    );
                });

                ui.add_space(22.0);
                section_label(ui, "CONNECTED DEVICES");
                ui.add_space(8.0);

                if self.device_order.is_empty() {
                    ui.label(egui::RichText::new("No controller detected").color(INK));
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new("Connect a gamepad to begin. The list refreshes automatically.")
                            .small()
                            .color(MUTED),
                    );
                } else {
                    let devices: Vec<(GamepadId, String)> = self
                        .device_order
                        .iter()
                        .filter_map(|id| self.devices.get(id).map(|device| (*id, device.name.clone())))
                        .collect();
                    for (id, name) in devices {
                        let selected = self.selected_device == Some(id);
                        let fill = if selected {
                            Color32::from_rgb(215, 230, 214)
                        } else {
                            Color32::TRANSPARENT
                        };
                        let response = ui.add_sized(
                            [ui.available_width(), 52.0],
                            egui::Button::new(
                                egui::RichText::new(format!("{name}\nDEVICE {id}"))
                                    .color(INK)
                                    .size(13.0),
                            )
                            .fill(fill)
                            .stroke(Stroke::new(1.0, if selected { GREEN } else { LINE })),
                        );
                        if response.clicked() {
                            self.selected_device = Some(id);
                        }
                        ui.add_space(6.0);
                    }
                }

                if let Some(error) = &self.initialization_error {
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new("Controller access could not start").strong().color(CORAL));
                    ui.label(egui::RichText::new(error).small().color(MUTED));
                    if ui.button("Retry controller access").clicked() {
                        self.initialize_input();
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Software-observed timing only. Physical latency needs external measurement.")
                            .small()
                            .color(MUTED),
                    );
                    ui.add_space(12.0);
                    ui.separator();
                    ui.label(egui::RichText::new("LOCAL SESSION").small().strong().color(MUTED));
                });
            });
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BACKGROUND).inner_margin(24.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("GAMEPAD DIAGNOSTICS").small().strong().color(GREEN));
                        ui.add_space(4.0);
                        ui.heading(egui::RichText::new("Live input signal").size(28.0).color(INK));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Reset session").clicked() {
                            self.clear_session();
                        }
                        ui.label(egui::RichText::new("APP-OBSERVED").small().strong().color(GREEN));
                    });
                });

                ui.add_space(18.0);
                ui.separator();

                let selected_name = self
                    .selected_state()
                    .map(|device| device.name.clone())
                    .unwrap_or_else(|| "No device selected".to_owned());
                let summary = self.selected_state().and_then(|device| device.intervals.summary());
                let event_count = self.selected_state().map_or(0, |device| device.event_count);
                let session_seconds = self.session_started.elapsed().as_secs_f64();

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    metric(ui, "ACTIVE DEVICE", &selected_name, "current selection");
                    metric(ui, "INPUT EVENTS", &event_count.to_string(), "this session");
                    metric(
                        ui,
                        "MEAN APP GAP",
                        &format_summary(summary, |stats| stats.mean_ms),
                        "between received events",
                    );
                    metric(
                        ui,
                        "P95 APP GAP",
                        &format_summary(summary, |stats| stats.p95_ms),
                        "recent interval samples",
                    );
                });

                ui.add_space(20.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("EVENT SPACING").small().strong().color(MUTED));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("SESSION {:02}:{:02}", (session_seconds / 60.0) as u64, session_seconds as u64 % 60))
                                .small()
                                .monospace()
                                .color(MUTED),
                        );
                    });
                });
                ui.add_space(8.0);

                let samples: Vec<f64> = self
                    .selected_state()
                    .map(|device| device.intervals.samples().collect())
                    .unwrap_or_default();
                draw_interval_chart(ui, &samples);

                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("Intervals are measured when this app processes input events; they are not controller polling or button-to-screen latency.")
                        .small()
                        .color(MUTED),
                );

                ui.add_space(22.0);
                ui.separator();
                ui.add_space(14.0);

                ui.columns(2, |columns| {
                    self.show_input_state(&mut columns[0]);
                    self.show_recent_events(&mut columns[1]);
                });
            });
    }

    fn show_input_state(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("CURRENT INPUT")
                .small()
                .strong()
                .color(GREEN),
        );
        ui.add_space(6.0);
        ui.heading(
            egui::RichText::new("Buttons and axes")
                .size(19.0)
                .color(INK),
        );
        ui.add_space(12.0);

        let Some(device) = self.selected_state() else {
            ui.label(
                egui::RichText::new("Choose a connected controller to inspect its input.")
                    .color(MUTED),
            );
            return;
        };

        ui.label(egui::RichText::new("BUTTONS").small().strong().color(MUTED));
        if device.buttons.is_empty() {
            ui.label(
                egui::RichText::new("Press a button to see it here.")
                    .small()
                    .color(MUTED),
            );
        } else {
            let mut buttons: Vec<_> = device.buttons.iter().collect();
            buttons.sort_by(|left, right| left.0.cmp(right.0));
            for (name, value) in buttons {
                ui.horizontal(|ui| {
                    status_dot(ui, *value > 0.5, 8.0);
                    ui.label(egui::RichText::new(name).color(INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(if *value > 0.5 { "HELD" } else { "IDLE" })
                                .small()
                                .strong()
                                .color(if *value > 0.5 { GREEN } else { MUTED }),
                        );
                    });
                });
            }
        }

        ui.add_space(14.0);
        ui.label(egui::RichText::new("AXES").small().strong().color(MUTED));
        if device.axes.is_empty() {
            ui.label(
                egui::RichText::new("Move a stick or trigger to see axis values.")
                    .small()
                    .color(MUTED),
            );
        } else {
            let mut axes: Vec<_> = device.axes.iter().collect();
            axes.sort_by(|left, right| left.0.cmp(right.0));
            for (name, value) in axes {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).color(INK));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!("{value:+.3}"))
                                .monospace()
                                .color(GREEN),
                        );
                    });
                });
                ui.add(egui::ProgressBar::new(((*value + 1.0) / 2.0).clamp(0.0, 1.0)).fill(GREEN));
            }
        }

        ui.add_space(14.0);
        ui.label(
            egui::RichText::new("LAST CHANGE")
                .small()
                .strong()
                .color(MUTED),
        );
        ui.label(egui::RichText::new(&device.last_event).color(INK));
    }

    fn show_recent_events(&self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("EVENT FEED")
                .small()
                .strong()
                .color(GREEN),
        );
        ui.add_space(6.0);
        ui.heading(egui::RichText::new("Recent activity").size(19.0).color(INK));
        ui.add_space(12.0);

        if self.recent_events.is_empty() {
            ui.label(
                egui::RichText::new("Input changes will appear here as they arrive.").color(MUTED),
            );
            return;
        }

        egui::ScrollArea::vertical()
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for event in &self.recent_events {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "+{:05.2}s",
                                event
                                    .received_at
                                    .duration_since(self.session_started)
                                    .as_secs_f64()
                            ))
                            .monospace()
                            .small()
                            .color(MUTED),
                        );
                        ui.label(
                            egui::RichText::new(&event.device_name)
                                .small()
                                .strong()
                                .color(GREEN),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.add_space(48.0);
                        ui.label(egui::RichText::new(&event.description).color(INK));
                        if let Some(interval_ms) = event.interval_ms {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{interval_ms:.2} ms"))
                                            .small()
                                            .monospace()
                                            .color(MUTED),
                                    );
                                },
                            );
                        }
                    });
                    ui.add_space(7.0);
                    ui.separator();
                }
            });
    }
}

impl eframe::App for GamepadApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_events();
        self.show_sidebar(ui);
        self.show_dashboard(ui);
        ui.ctx().request_repaint_after(Duration::from_millis(8));
    }
}

fn section_label(ui: &mut egui::Ui, label: &str) {
    ui.label(egui::RichText::new(label).small().strong().color(MUTED));
}

fn status_dot(ui: &mut egui::Ui, active: bool, size: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    ui.painter().circle_filled(
        rect.center(),
        size * 0.42,
        if active {
            GREEN
        } else {
            Color32::from_rgb(177, 188, 179)
        },
    );
}

fn metric(ui: &mut egui::Ui, label: &str, value: &str, detail: &str) {
    ui.vertical(|ui| {
        ui.set_min_width((ui.available_width() / 4.0).max(110.0));
        ui.label(egui::RichText::new(label).small().strong().color(MUTED));
        ui.add_space(3.0);
        ui.label(egui::RichText::new(value).size(19.0).strong().color(INK));
        ui.label(egui::RichText::new(detail).small().color(MUTED));
    });
    ui.add_space(14.0);
}

fn format_summary(
    summary: Option<IntervalSummary>,
    select: impl FnOnce(IntervalSummary) -> f64,
) -> String {
    summary
        .map(|stats| format!("{:.2} ms", select(stats)))
        .unwrap_or_else(|| "--".to_owned())
}

fn draw_interval_chart(ui: &mut egui::Ui, samples: &[f64]) {
    let width = ui.available_width().max(100.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 148.0), Sense::hover());
    let painter = ui.painter_at(rect);

    for row in 0..4 {
        let y = rect.top() + rect.height() * row as f32 / 3.0;
        painter.line_segment(
            [Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)],
            Stroke::new(1.0, LINE),
        );
    }

    if samples.is_empty() {
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "Waiting for input events",
            FontId::proportional(14.0),
            MUTED,
        );
        return;
    }

    let visible = &samples[samples.len().saturating_sub(96)..];
    let max_value = visible.iter().copied().fold(0.0_f64, f64::max).max(1.0);
    let points: Vec<Pos2> = visible
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let x = if visible.len() <= 1 {
                rect.center().x
            } else {
                rect.left() + rect.width() * index as f32 / (visible.len() - 1) as f32
            };
            let normalized = (*value / max_value).clamp(0.0, 1.0) as f32;
            let y = rect.bottom() - normalized * rect.height();
            Pos2::new(x, y)
        })
        .collect();

    if points.len() == 1 {
        painter.circle_filled(points[0], 3.5, GREEN);
    } else {
        painter.add(egui::Shape::line(points, Stroke::new(2.0, GREEN)));
    }

    painter.text(
        Pos2::new(rect.right() - 3.0, rect.top() + 4.0),
        Align2::RIGHT_TOP,
        format!("{max_value:.1} ms max in view"),
        FontId::proportional(11.0),
        MUTED,
    );
    painter.circle_filled(
        Pos2::new(rect.right() - 8.0, rect.bottom() - 6.0),
        3.0,
        CORAL,
    );
    painter.text(
        Pos2::new(rect.right() - 16.0, rect.bottom() - 6.0),
        Align2::RIGHT_CENTER,
        format!("{} samples", samples.len()),
        FontId::proportional(11.0),
        MUTED,
    );
}
