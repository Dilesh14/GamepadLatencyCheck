use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, RichText, Sense, Stroke, Vec2,
    pos2, vec2,
};
use gilrs::GamepadId;

use crate::export::{self, ExportRequest};
use crate::input::{self, BackendStatus, ControllerView, InputMonitor};
use crate::reaction::{Phase, ReactionSession, TrialResult};

const BG: Color32 = Color32::from_rgb(15, 17, 22);
const PANEL_BG: Color32 = Color32::from_rgb(21, 24, 32);
const CARD_BG: Color32 = Color32::from_rgb(27, 31, 41);
const STAGE_BG: Color32 = Color32::from_rgb(18, 21, 28);
const TEXT: Color32 = Color32::from_rgb(228, 232, 240);
const MUTED: Color32 = Color32::from_rgb(148, 158, 176);
const FAINT: Color32 = Color32::from_rgb(103, 113, 132);
const ACCENT: Color32 = Color32::from_rgb(94, 234, 212);
const ACCENT_DIM: Color32 = Color32::from_rgb(31, 92, 86);
const GREEN: Color32 = Color32::from_rgb(74, 222, 128);
const AMBER: Color32 = Color32::from_rgb(251, 191, 36);
const RED: Color32 = Color32::from_rgb(248, 113, 113);

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Diagnostics,
    Reaction,
}

struct Feedback {
    ok: bool,
    text: String,
}

pub struct LatencyApp {
    tab: Tab,
    input: InputMonitor,
    reaction: ReactionSession,
    reaction_controller: Option<GamepadId>,
    export_feedback: Option<Feedback>,
}

impl LatencyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = PANEL_BG;
        visuals.window_fill = PANEL_BG;
        visuals.extreme_bg_color = BG;
        visuals.faint_bg_color = CARD_BG;
        visuals.selection.bg_fill = ACCENT_DIM;
        visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        visuals.hyperlink_color = ACCENT;
        cc.egui_ctx.set_visuals(visuals);

        let mut style = (*cc.egui_ctx.style_of(egui::Theme::Dark)).clone();
        style.spacing.item_spacing = vec2(8.0, 8.0);
        style.spacing.button_padding = vec2(10.0, 5.0);
        cc.egui_ctx.set_style_of(egui::Theme::Dark, style.clone());
        cc.egui_ctx.set_style_of(egui::Theme::Light, style);

        Self {
            tab: Tab::Diagnostics,
            input: InputMonitor::new(),
            reaction: ReactionSession::new(),
            reaction_controller: None,
            export_feedback: None,
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new("Gamepad Latency Check")
                        .size(20.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    RichText::new("Software-observed input timing diagnostics")
                        .size(11.0)
                        .color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.selectable_value(&mut self.tab, Tab::Reaction, "Reaction test");
                ui.selectable_value(&mut self.tab, Tab::Diagnostics, "Diagnostics");
            });
        });
    }

    fn sidebar_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        ui.label(
            RichText::new("CONTROLLERS")
                .size(10.5)
                .strong()
                .color(MUTED),
        );
        ui.add_space(2.0);

        let status = self.input.status().clone();
        match status {
            BackendStatus::Starting => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new("Starting input service...").color(MUTED));
                });
            }
            BackendStatus::Failed(error) => {
                ui.label(
                    RichText::new("Gamepad input is unavailable.")
                        .strong()
                        .color(RED),
                );
                ui.label(RichText::new(error).size(11.5).color(MUTED));
                ui.label(
                    RichText::new(
                        "Check that the controller is connected and that this app has permission to read it.",
                    )
                    .size(11.0)
                    .color(FAINT),
                );
                if ui.button("Retry input service").clicked() {
                    self.input.retry();
                    self.export_feedback = None;
                }
            }
            BackendStatus::Running => {}
        }

        let controllers = self.input.controller_list();
        if controllers.is_empty() {
            ui.add_space(6.0);
            ui.label(RichText::new("No controllers detected").color(MUTED));
            ui.label(
                RichText::new(
                    "Connect a controller over USB or Bluetooth. It should appear here automatically.",
                )
                .size(11.0)
                .color(FAINT),
            );
        } else {
            ui.add_space(4.0);
            let mut chosen = None;
            for item in controllers {
                let selected = self.input.selected_id() == Some(item.id);
                let status = if item.connected {
                    item.power.as_str()
                } else {
                    "Disconnected"
                };
                let color = if item.connected { TEXT } else { AMBER };
                let text = RichText::new(format!("{}\n{} · {}", item.name, item.id, status))
                    .size(12.5)
                    .color(color);

                ui.push_id(usize::from(item.id), |ui| {
                    if ui
                        .add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::selectable(selected, text),
                        )
                        .clicked()
                    {
                        chosen = Some(item.id);
                    }
                });
            }
            if let Some(id) = chosen {
                self.input.select(id);
                self.export_feedback = None;
            }
        }

        ui.add_space(8.0);
        ui.label(
            RichText::new(
                "Windows: Game Input may require this window to be focused while reading controller events.",
            )
            .size(10.5)
            .color(FAINT),
        );

        ui.with_layout(Layout::bottom_up(Align::LEFT), |ui| {
            ui.add_space(4.0);
            ui.label(
                RichText::new(format!(
                    "v{} · {} {} · input polled every ~1 ms",
                    APP_VERSION,
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ))
                .size(10.0)
                .color(FAINT),
            );
        });
    }

    fn diagnostics_ui(&mut self, ui: &mut egui::Ui) {
        let Some(view) = self.input.selected_view() else {
            self.diagnostics_empty_ui(ui);
            return;
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.diagnostics_content_ui(ui, view, Instant::now())
            });
    }

    fn diagnostics_content_ui(&mut self, ui: &mut egui::Ui, view: ControllerView, now: Instant) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(&view.info.name)
                        .size(22.0)
                        .strong()
                        .color(TEXT),
                );
                ui.label(
                    RichText::new(format!(
                        "Gamepad {} · {} · {}",
                        view.id, view.info.mapping, view.info.power
                    ))
                    .size(12.0)
                    .color(MUTED),
                );
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                status_chip(ui, view.connected);
                let last = view
                    .last_event_at
                    .map(|at| {
                        format!(
                            "{:.1} s since last event",
                            now.duration_since(at).as_secs_f64()
                        )
                    })
                    .unwrap_or_else(|| "No events yet".to_owned());
                ui.label(RichText::new(last).size(11.0).color(MUTED));
                ui.label(
                    RichText::new(format!("{} events", view.event_count))
                        .size(11.0)
                        .color(MUTED),
                );
            });
        });

        if !view.connected {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "This controller is disconnected. Reconnect it to continue reading input; retained samples remain available for export.",
                )
                .color(AMBER)
                .size(12.0),
            );
        }
        if !view.info.mapping_supported {
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "This device does not report a standard gamepad mapping, so some buttons or axes may be missing or mislabeled.",
                )
                .color(AMBER)
                .size(12.0),
            );
        }

        ui.add_space(6.0);
        let mut reset = false;
        let mut export = false;
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!view.samples.is_empty(), egui::Button::new("Reset samples"))
                .clicked()
            {
                reset = true;
            }
            if ui
                .add_enabled(!view.samples.is_empty(), egui::Button::new("Export CSV"))
                .on_hover_text(
                    "Saves the retained interval window with session metadata. Metadata lines start with #.",
                )
                .clicked()
            {
                export = true;
            }
            ui.label(
                RichText::new(format!(
                    "Retained window: {} / {} intervals",
                    view.samples.len(),
                    crate::stats::WINDOW_CAPACITY
                ))
                .size(11.0)
                .color(FAINT),
            );
        });

        if let Some(feedback) = &self.export_feedback {
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(if feedback.ok {
                        "CSV saved"
                    } else {
                        "Export failed"
                    })
                    .strong()
                    .color(if feedback.ok { GREEN } else { RED }),
                );
                ui.label(
                    RichText::new(&feedback.text)
                        .size(11.0)
                        .color(MUTED)
                        .monospace(),
                );
                if feedback.ok && ui.small_button("Copy path").clicked() {
                    ui.ctx().copy_text(feedback.text.clone());
                }
            });
        }

        ui.add_space(8.0);
        let cards = [
            ("Samples", format!("{}", view.samples.len())),
            ("Min", fmt_ms(view.summary.map(|s| s.min_ms))),
            ("Mean", fmt_ms(view.summary.map(|s| s.mean_ms))),
            ("Median", fmt_ms(view.summary.map(|s| s.median_ms))),
            ("P95", fmt_ms(view.summary.map(|s| s.p95_ms))),
            ("Max", fmt_ms(view.summary.map(|s| s.max_ms))),
        ];
        ui.horizontal_wrapped(|ui| {
            for (label, value) in cards {
                stat_card(ui, label, &value, 128.0, 62.0);
            }
        });

        ui.add_space(8.0);
        interval_plot(ui, &view.samples);

        ui.add_space(10.0);
        ui.label(RichText::new("BUTTONS").size(10.5).strong().color(MUTED));
        ui.add_space(2.0);
        egui::Grid::new("buttons_grid")
            .num_columns(3)
            .spacing(vec2(18.0, 6.0))
            .show(ui, |ui| {
                for (index, (button, value)) in view.buttons.iter().enumerate() {
                    let (symbol, color) = button_state(*value);
                    let mut text = format!("{symbol} {}", input::button_label(*button));
                    if *value > 0.02 && *value < 0.98 {
                        text.push_str(&format!(" · {:.0}%", value * 100.0));
                    }
                    ui.label(RichText::new(text).size(12.0).color(color));
                    if (index + 1) % 3 == 0 {
                        ui.end_row();
                    }
                }
            });

        ui.add_space(10.0);
        ui.label(RichText::new("AXES").size(10.5).strong().color(MUTED));
        ui.add_space(2.0);
        for (axis, value) in &view.axes {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [150.0, 18.0],
                    egui::Label::new(
                        RichText::new(input::axis_label(*axis))
                            .size(12.0)
                            .color(MUTED),
                    ),
                );
                axis_bar(ui, *value);
                ui.monospace(RichText::new(format!("{value:+.2}")).size(11.5).color(TEXT));
            });
        }

        ui.add_space(10.0);
        ui.label(
            RichText::new(format!(
                "Device UUID {} · Vendor {} · Product {}",
                if view.info.uuid.is_empty() {
                    "unknown"
                } else {
                    &view.info.uuid
                },
                view.info
                    .vendor_id
                    .map(|id| format!("0x{id:04x}"))
                    .unwrap_or_else(|| "unknown".to_owned()),
                view.info
                    .product_id
                    .map(|id| format!("0x{id:04x}"))
                    .unwrap_or_else(|| "unknown".to_owned()),
            ))
            .size(10.0)
            .color(FAINT),
        );
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Intervals are the time between consecutive input events as this app processes them, using a monotonic clock. They are not controller polling rate and not physical button-to-screen latency. Idle gaps between presses are included. Event queueing, OS scheduling, focus, transport, and mappings can affect what is observed.",
            )
            .size(10.5)
            .color(FAINT),
        );

        if reset {
            self.input.reset_selected_samples();
            self.export_feedback = None;
        }
        if export {
            self.export_selected();
        }
    }

    fn diagnostics_empty_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(70.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("No controller selected").size(24.0).strong().color(TEXT));
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Connect a gamepad over USB or Bluetooth. Detected controllers appear in the sidebar and are selected automatically.",
                )
                .color(MUTED),
            );
            ui.add_space(14.0);
            ui.label(
                RichText::new(
                    "macOS: Input Monitoring permission may be required.\nLinux: Your user may need read access to /dev/input devices.\nWindows: Keep this window focused while reading controller events.",
                )
                .size(11.5)
                .color(FAINT),
            );
            if matches!(self.input.status(), BackendStatus::Failed(_))
                && ui.button("Retry input service").clicked()
            {
                self.input.retry();
            }
        });
    }

    fn reaction_ui(&mut self, ui: &mut egui::Ui) {
        let view = self.input.selected_view();
        match self.reaction.phase() {
            Phase::Idle | Phase::Complete => self.reaction_intro_ui(ui, view),
            _ => self.reaction_active_ui(ui, view),
        }
    }

    fn reaction_intro_ui(&mut self, ui: &mut egui::Ui, view: Option<ControllerView>) {
        let mut start_clicked = false;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                ui.label(RichText::new("Reaction test").size(26.0).strong().color(TEXT));
                ui.label(
                    RichText::new(
                        "Press any button on the selected controller as soon as the circle or square appears.",
                    )
                    .color(MUTED),
                );

                ui.add_space(12.0);
                match &view {
                    Some(view) if view.connected => {
                        ui.label(
                            RichText::new(format!("Selected controller: {}", view.info.name))
                                .color(GREEN)
                                .strong(),
                        );
                    }
                    Some(view) => {
                        ui.label(
                            RichText::new(format!(
                                "Selected controller: {} (disconnected)",
                                view.info.name
                            ))
                            .color(AMBER)
                            .strong(),
                        );
                    }
                    None => {
                        ui.label(
                            RichText::new("Select a connected controller in the sidebar.")
                                .color(AMBER)
                                .strong(),
                        );
                    }
                }

                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label(RichText::new("Session rules").strong().color(TEXT));
                    for line in [
                        "20 successful trials per session.",
                        "Each trial waits a random 1.5 to 4 seconds before showing the cue.",
                        "Press any digital button on the selected controller after the cue appears.",
                        "A press before the cue is a false start. It does not count and the trial restarts.",
                        "Disconnecting or changing controllers pauses the session. Resume to restart the current trial.",
                    ] {
                        ui.label(RichText::new(format!("• {line}")).size(12.5).color(MUTED));
                    }
                });

                ui.add_space(10.0);
                ui.label(
                    RichText::new(
                        "Measured value: app-observed visual reaction time, from cue submission for drawing to input receipt. It includes human response, event queueing, and frame/display scheduling. It is not controller hardware latency and not photon-to-input time. Results stay in memory for this app session and are not included in the diagnostics CSV export.",
                    )
                    .size(11.0)
                    .color(FAINT),
                );

                if self.reaction.is_complete() {
                    ui.add_space(14.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(RichText::new("Last session").size(16.0).strong().color(TEXT));
                    self.reaction_summary_ui(ui, true);
                    ui.add_space(10.0);
                }

                ui.add_space(14.0);
                let can_start = view
                    .as_ref()
                    .map(|view| view.connected)
                    .unwrap_or(false);
                if ui
                    .add_enabled(
                        can_start,
                        egui::Button::new(RichText::new("Start session").strong()),
                    )
                    .clicked()
                {
                    start_clicked = true;
                }
                if !can_start {
                    ui.label(
                        RichText::new("Connect and select a controller to start.")
                            .size(11.0)
                            .color(FAINT),
                    );
                }
            });

        if start_clicked {
            self.start_reaction();
        }
    }

    fn reaction_active_ui(&mut self, ui: &mut egui::Ui, view: Option<ControllerView>) {
        let phase = self.reaction.phase();
        let paused = phase == Phase::Paused;
        let total = self.reaction.target();
        let completed = self.reaction.trials().len();
        let fraction = completed as f32 / total as f32;

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::ProgressBar::new(fraction)
                    .desired_width(260.0)
                    .text(format!(
                        "Trial {} of {}",
                        self.reaction.current_trial_number(),
                        total
                    )),
            );
            if completed > 0 {
                ui.label(
                    RichText::new(format!("{completed} recorded"))
                        .size(11.5)
                        .color(MUTED),
                );
            }
            ui.label(
                RichText::new(format!(
                    "{} false start{}",
                    self.reaction.false_starts(),
                    if self.reaction.false_starts() == 1 {
                        ""
                    } else {
                        "s"
                    }
                ))
                .size(11.5)
                .color(MUTED),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let name = self
                    .reaction
                    .controller_name()
                    .map(str::to_owned)
                    .or_else(|| view.as_ref().map(|view| view.info.name.clone()))
                    .unwrap_or_else(|| "Controller".to_owned());
                let connected = view.as_ref().map(|view| view.connected).unwrap_or(false);
                ui.label(
                    RichText::new(if connected {
                        name
                    } else {
                        format!("{name} (disconnected)")
                    })
                    .size(11.5)
                    .color(if connected { MUTED } else { AMBER }),
                );
            });
        });

        let footer_space = if paused { 190.0 } else { 210.0 };
        let stage_height = (ui.available_height() - footer_space).max(200.0);
        let (stage, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), stage_height), Sense::hover());
        paint_reaction_stage(ui, stage, &self.reaction);

        ui.add_space(8.0);
        if paused {
            ui.horizontal(|ui| {
                let can_resume = view.as_ref().map(|view| view.connected).unwrap_or(false);
                if ui
                    .add_enabled(
                        can_resume,
                        egui::Button::new(RichText::new("Resume session").strong()),
                    )
                    .clicked()
                {
                    self.resume_reaction();
                }
                if ui.button("End session").clicked() {
                    self.end_reaction();
                }
                if !can_resume {
                    ui.label(
                        RichText::new("Reconnect and select a controller to resume.")
                            .size(11.0)
                            .color(AMBER),
                    );
                }
            });
            if let Some(reason) = self.reaction.pause_reason() {
                ui.label(RichText::new(reason).size(11.0).color(FAINT));
            }
        } else if ui.button("End session").clicked() {
            self.end_reaction();
        }

        ui.add_space(4.0);
        self.reaction_summary_ui(ui, false);

        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "App-observed reaction time from cue submission to input receipt. Includes human response, event queueing, and frame/display scheduling; not controller hardware latency or photon-to-input time.",
            )
            .size(10.0)
            .color(FAINT),
        );
    }

    fn reaction_summary_ui(&self, ui: &mut egui::Ui, _complete: bool) {
        let summary = self.reaction.stats();
        ui.horizontal_wrapped(|ui| {
            stat_card(
                ui,
                "Trials",
                &format!(
                    "{} / {}",
                    self.reaction.trials().len(),
                    self.reaction.target()
                ),
                96.0,
                56.0,
            );
            stat_card(ui, "Mean", &fmt_ms(summary.map(|s| s.mean_ms)), 110.0, 56.0);
            stat_card(ui, "P95", &fmt_ms(summary.map(|s| s.p95_ms)), 110.0, 56.0);
            stat_card(ui, "Best", &fmt_ms(summary.map(|s| s.min_ms)), 110.0, 56.0);
            stat_card(
                ui,
                "False starts",
                &format!("{}", self.reaction.false_starts()),
                100.0,
                56.0,
            );
        });

        if !self.reaction.trials().is_empty() {
            ui.add_space(6.0);
            ui.label(
                RichText::new("Recent trials")
                    .size(10.5)
                    .strong()
                    .color(MUTED),
            );
            egui::Grid::new("recent_trials")
                .num_columns(3)
                .spacing(vec2(22.0, 4.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Trial").size(10.5).color(FAINT));
                    ui.label(RichText::new("Cue").size(10.5).color(FAINT));
                    ui.label(RichText::new("Reaction").size(10.5).color(FAINT));
                    ui.end_row();

                    for (index, trial) in self.reaction.trials().iter().enumerate().rev().take(10) {
                        ui.label(
                            RichText::new(format!("#{}", index + 1))
                                .size(11.5)
                                .color(MUTED),
                        );
                        ui.label(
                            RichText::new(shape_symbol(trial.shape))
                                .size(11.5)
                                .color(TEXT),
                        );
                        ui.label(
                            RichText::new(fmt_reaction(trial))
                                .size(11.5)
                                .monospace()
                                .color(TEXT),
                        );
                        ui.end_row();
                    }
                });
        }
    }

    fn start_reaction(&mut self) {
        let mut selected = None;
        if let Some(id) = self.input.selected_id() {
            if let Some(controller) = self.input.selected_controller() {
                if controller.connected {
                    selected = Some((id, controller.info.name.clone()));
                }
            }
        }
        if let Some((id, name)) = selected {
            self.reaction.start(&name, Instant::now());
            self.reaction_controller = Some(id);
        }
    }

    fn resume_reaction(&mut self) {
        let mut selected = None;
        if let Some(id) = self.input.selected_id() {
            if let Some(controller) = self.input.selected_controller() {
                if controller.connected {
                    selected = Some((id, controller.info.name.clone()));
                }
            }
        }
        if let Some((id, name)) = selected {
            self.reaction.resume(&name, Instant::now());
            self.reaction_controller = Some(id);
        }
    }

    fn end_reaction(&mut self) {
        self.reaction.reset();
        self.reaction_controller = None;
    }

    fn export_selected(&mut self) {
        let Some(controller) = self.input.selected_controller() else {
            return;
        };

        let controller_name = controller.info.name.clone();
        let mapping = controller.info.mapping.clone();
        let power = controller.info.power.clone();
        let uuid = controller.info.uuid.clone();
        let connected = controller.connected;
        let samples: Vec<_> = controller.intervals().iter().copied().collect();

        if samples.is_empty() {
            return;
        }

        let request = ExportRequest {
            app_version: APP_VERSION,
            controller_name: &controller_name,
            controller_id: controller_id_string(self.input.selected_id()),
            connected,
            mapping: &mapping,
            power: &power,
            uuid: &uuid,
            samples: &samples,
            exported_at: SystemTime::now(),
            exported_instant: Instant::now(),
        };

        match export::write_csv(&request) {
            Ok(path) => {
                self.export_feedback = Some(Feedback {
                    ok: true,
                    text: path.display().to_string(),
                });
            }
            Err(error) => {
                self.export_feedback = Some(Feedback {
                    ok: false,
                    text: error,
                });
            }
        }
    }
}

impl eframe::App for LatencyApp {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.input.poll();

        if self.reaction.is_active() {
            if self.tab != Tab::Reaction {
                self.reaction.pause(
                    "The session was paused because the Reaction test tab is not visible. Return to the Reaction test tab and resume to restart the current trial.",
                );
            }

            let bound = self.reaction_controller;
            let invalid = match bound {
                Some(id) => !self.input.is_connected(id) || self.input.selected_id() != Some(id),
                None => true,
            };
            if invalid && self.reaction.is_active() {
                self.reaction.pause(
                    "The session was paused because the selected controller is unavailable or a different controller is selected. Reconnect, select a controller, and resume to restart the current trial.",
                );
            }
        }

        // Handle presses before advancing time so a press that arrived while the
        // app was busy is judged against the phase the user actually saw.
        let presses = self.input.presses().to_vec();
        for (id, at) in presses {
            if Some(id) == self.reaction_controller {
                self.reaction.press(at);
            }
        }

        self.reaction.tick(Instant::now());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("header").show(ui, |ui| self.header_ui(ui));
        egui::Panel::left("controllers")
            .default_size(272.0)
            .min_size(220.0)
            .show(ui, |ui| self.sidebar_ui(ui));
        egui::CentralPanel::default().show(ui, |ui| match self.tab {
            Tab::Diagnostics => self.diagnostics_ui(ui),
            Tab::Reaction => self.reaction_ui(ui),
        });

        let interval = if self.reaction.is_active() || self.reaction.is_paused() {
            Duration::from_millis(4)
        } else {
            Duration::from_millis(16)
        };
        ui.ctx().request_repaint_after(interval);
    }
}

fn controller_id_string(id: Option<GamepadId>) -> String {
    id.map(|id| id.to_string())
        .unwrap_or_else(|| "none".to_owned())
}

fn status_chip(ui: &mut egui::Ui, connected: bool) {
    let (label, color) = if connected {
        (" Connected ", GREEN)
    } else {
        (" Disconnected ", AMBER)
    };
    ui.label(
        RichText::new(label)
            .size(11.0)
            .strong()
            .color(Color32::BLACK)
            .background_color(color),
    );
}

fn button_state(value: f32) -> (&'static str, Color32) {
    if value > 0.5 {
        ("●", GREEN)
    } else if value > 0.02 {
        ("◐", AMBER)
    } else {
        ("○", FAINT)
    }
}

fn stat_card(ui: &mut egui::Ui, label: &str, value: &str, width: f32, height: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), CARD_BG);
    painter.text(
        rect.left_top() + vec2(11.0, 9.0),
        Align2::LEFT_TOP,
        label,
        FontId::proportional(10.5),
        MUTED,
    );
    painter.text(
        rect.left_bottom() + vec2(11.0, -9.0),
        Align2::LEFT_BOTTOM,
        value,
        FontId::proportional(19.0),
        TEXT,
    );
}

fn axis_bar(ui: &mut egui::Ui, value: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(260.0, 12.0), Sense::hover());
    let painter = ui.painter();
    let radius = CornerRadius::same(3);
    painter.rect_filled(rect, radius, CARD_BG);

    let inner = egui::Rect::from_min_max(rect.min + vec2(2.0, 2.0), rect.max - vec2(2.0, 2.0));
    let center_x = inner.center().x;
    painter.vline(
        center_x,
        inner.y_range(),
        Stroke::new(1.0, Color32::from_rgb(58, 64, 78)),
    );

    let width = inner.width() * 0.5 * value.clamp(-1.0, 1.0);
    if width.abs() > 0.5 {
        let fill = if width >= 0.0 {
            egui::Rect::from_min_max(
                pos2(center_x, inner.top() + 1.0),
                pos2(center_x + width, inner.bottom() - 1.0),
            )
        } else {
            egui::Rect::from_min_max(
                pos2(center_x + width, inner.top() + 1.0),
                pos2(center_x, inner.bottom() - 1.0),
            )
        };
        painter.rect_filled(fill, CornerRadius::same(2), ACCENT);
    }
}

fn interval_plot(ui: &mut egui::Ui, samples: &[crate::stats::IntervalSample]) {
    let width = ui.available_width().max(240.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, 92.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(8), CARD_BG);

    let inner = egui::Rect::from_min_max(
        rect.left_top() + vec2(10.0, 24.0),
        rect.right_bottom() - vec2(10.0, 10.0),
    );

    painter.text(
        rect.left_top() + vec2(10.0, 7.0),
        Align2::LEFT_TOP,
        "Recent intervals",
        FontId::proportional(10.5),
        MUTED,
    );

    if samples.is_empty() {
        painter.text(
            inner.center(),
            Align2::CENTER_CENTER,
            "No intervals yet — move a stick or press a button.",
            FontId::proportional(11.5),
            FAINT,
        );
        return;
    }

    let shown = samples.len().min(160);
    let recent = &samples[samples.len() - shown..];
    let max_ms = recent
        .iter()
        .map(|sample| sample.interval.as_secs_f64() * 1000.0)
        .fold(1.0_f64, f64::max);

    painter.text(
        rect.right_top() + vec2(-10.0, 7.0),
        Align2::RIGHT_TOP,
        format!("max {max_ms:.2} ms · last {shown}"),
        FontId::proportional(10.5),
        FAINT,
    );

    painter.hline(
        inner.x_range(),
        inner.bottom(),
        Stroke::new(1.0, Color32::from_rgb(48, 54, 67)),
    );

    let bar_width = inner.width() / shown as f32;
    for (index, sample) in recent.iter().enumerate() {
        let value = sample.interval.as_secs_f64() * 1000.0;
        let height = (value / max_ms * inner.height() as f64) as f32;
        if height < 0.5 {
            continue;
        }
        let x = inner.left() + index as f32 * bar_width;
        let bar = egui::Rect::from_min_max(
            pos2(x + 0.5, inner.bottom() - height),
            pos2(x + (bar_width - 1.0).max(1.0), inner.bottom() - 1.0),
        );
        painter.rect_filled(bar, CornerRadius::same(1), ACCENT_DIM);
    }
}

fn paint_reaction_stage(ui: &egui::Ui, rect: egui::Rect, session: &ReactionSession) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(12), STAGE_BG);

    let center = rect.center();
    match session.phase() {
        Phase::Waiting => {
            painter.text(
                center - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                "Wait for the cue...",
                FontId::proportional(25.0),
                MUTED,
            );
            painter.text(
                center + vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "A circle or square appears after a random 1.5–4 second wait.",
                FontId::proportional(12.0),
                FAINT,
            );
        }
        Phase::Cue => {
            let size = (rect.width().min(rect.height()) * 0.34).clamp(70.0, 220.0);
            match session.cue_shape() {
                crate::reaction::CueShape::Circle => {
                    painter.circle_filled(center, size * 0.5, ACCENT);
                }
                crate::reaction::CueShape::Square => {
                    painter.rect_filled(
                        egui::Rect::from_center_size(center, Vec2::splat(size)),
                        CornerRadius::same(6),
                        ACCENT,
                    );
                }
            }
            painter.text(
                center + vec2(0.0, size * 0.5 + 24.0),
                Align2::CENTER_TOP,
                "Press any button",
                FontId::proportional(13.0),
                MUTED,
            );
        }
        Phase::FalseStart => {
            painter.text(
                center - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                "False start",
                FontId::proportional(28.0),
                AMBER,
            );
            painter.text(
                center + vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "You pressed before the cue. This trial restarts.",
                FontId::proportional(12.5),
                MUTED,
            );
        }
        Phase::InterTrial => {
            let last = session.trials().last();
            painter.text(
                center - vec2(0.0, 12.0),
                Align2::CENTER_CENTER,
                last.map(fmt_reaction)
                    .unwrap_or_else(|| "Recorded".to_owned()),
                FontId::proportional(40.0),
                TEXT,
            );
            painter.text(
                center + vec2(0.0, 32.0),
                Align2::CENTER_CENTER,
                "Trial recorded. The next cue is coming up...",
                FontId::proportional(12.5),
                MUTED,
            );
        }
        Phase::Paused => {
            painter.text(
                center - vec2(0.0, 8.0),
                Align2::CENTER_CENTER,
                "Session paused",
                FontId::proportional(28.0),
                AMBER,
            );
            painter.text(
                center + vec2(0.0, 24.0),
                Align2::CENTER_CENTER,
                "Reconnect and select a controller, then resume.",
                FontId::proportional(12.5),
                MUTED,
            );
        }
        Phase::Idle | Phase::Complete => {
            painter.text(
                center,
                Align2::CENTER_CENTER,
                "No active session",
                FontId::proportional(20.0),
                FAINT,
            );
        }
    }
}

fn fmt_ms(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.2} ms"))
        .unwrap_or_else(|| "—".to_owned())
}

fn fmt_reaction(trial: &TrialResult) -> String {
    format!("{:.1} ms", trial.reaction.as_secs_f64() * 1000.0)
}

fn shape_symbol(shape: crate::reaction::CueShape) -> &'static str {
    match shape {
        crate::reaction::CueShape::Circle => "● Circle",
        crate::reaction::CueShape::Square => "■ Square",
    }
}
