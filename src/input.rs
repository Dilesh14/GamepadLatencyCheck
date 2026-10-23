//! Background gamepad input polling.
//!
//! Input is polled on a dedicated thread so event timestamps are captured close
//! to arrival instead of once per rendered frame. The UI drains a channel and
//! keeps the latest state for every controller.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use gilrs::{Axis, Button, EventType, GamepadId, Gilrs, MappingSource, PowerInfo};

use crate::stats::{EventKind, IntervalSample, IntervalWindow};

/// Polling cadence of the input thread.
const INPUT_POLL_INTERVAL: Duration = Duration::from_millis(1);
/// How often controller metadata (name, battery, mapping) is refreshed.
const INFO_REFRESH_INTERVAL: Duration = Duration::from_secs(2);

pub const BUTTON_ORDER: [Button; 19] = [
    Button::South,
    Button::East,
    Button::West,
    Button::North,
    Button::LeftTrigger,
    Button::RightTrigger,
    Button::LeftTrigger2,
    Button::RightTrigger2,
    Button::Select,
    Button::Start,
    Button::Mode,
    Button::LeftThumb,
    Button::RightThumb,
    Button::DPadUp,
    Button::DPadDown,
    Button::DPadLeft,
    Button::DPadRight,
    Button::C,
    Button::Z,
];

pub const AXIS_ORDER: [Axis; 8] = [
    Axis::LeftStickX,
    Axis::LeftStickY,
    Axis::RightStickX,
    Axis::RightStickY,
    Axis::LeftZ,
    Axis::RightZ,
    Axis::DPadX,
    Axis::DPadY,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ControllerInfo {
    pub name: String,
    pub os_name: String,
    pub uuid: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub power: String,
    pub mapping: String,
    pub mapping_supported: bool,
}

impl ControllerInfo {
    fn unknown(id: GamepadId) -> Self {
        Self {
            name: format!("Controller {id}"),
            os_name: String::new(),
            uuid: String::new(),
            vendor_id: None,
            product_id: None,
            power: "Battery unknown".to_owned(),
            mapping: "Unknown mapping".to_owned(),
            mapping_supported: false,
        }
    }

    fn from_gamepad(gamepad: &gilrs::Gamepad<'_>) -> Self {
        let mapping_source = gamepad.mapping_source();
        Self {
            name: gamepad.name().to_owned(),
            os_name: gamepad.os_name().to_owned(),
            uuid: format_uuid(&gamepad.uuid()),
            vendor_id: gamepad.vendor_id(),
            product_id: gamepad.product_id(),
            power: power_label(gamepad.power_info()),
            mapping: mapping_label(mapping_source).to_owned(),
            mapping_supported: mapping_source != MappingSource::None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum InputEventKind {
    ButtonPressed(Button),
    ButtonReleased(Button),
    ButtonChanged(Button, f32),
    AxisChanged(Axis, f32),
}

impl InputEventKind {
    fn event_kind(self) -> EventKind {
        match self {
            Self::ButtonPressed(_) | Self::ButtonReleased(_) | Self::ButtonChanged(_, _) => {
                EventKind::Button
            }
            Self::AxisChanged(_, _) => EventKind::Axis,
        }
    }
}

enum InputMessage {
    Ready,
    Error(String),
    Controller {
        id: GamepadId,
        info: ControllerInfo,
    },
    Disconnected {
        id: GamepadId,
    },
    Event {
        id: GamepadId,
        at: Instant,
        kind: InputEventKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendStatus {
    Starting,
    Running,
    Failed(String),
}

pub struct Controller {
    pub info: ControllerInfo,
    pub connected: bool,
    buttons: HashMap<Button, f32>,
    axes: HashMap<Axis, f32>,
    intervals: IntervalWindow,
    last_event_at: Option<Instant>,
    event_count: u64,
}

impl Controller {
    fn new(info: ControllerInfo) -> Self {
        let mut buttons = HashMap::with_capacity(BUTTON_ORDER.len());
        for button in BUTTON_ORDER {
            buttons.insert(button, 0.0);
        }
        let mut axes = HashMap::with_capacity(AXIS_ORDER.len());
        for axis in AXIS_ORDER {
            axes.insert(axis, 0.0);
        }

        Self {
            info,
            connected: true,
            buttons,
            axes,
            intervals: IntervalWindow::new(),
            last_event_at: None,
            event_count: 0,
        }
    }

    fn apply(&mut self, kind: InputEventKind, at: Instant) {
        match kind {
            InputEventKind::ButtonPressed(button) => {
                self.set_button(button, 1.0);
            }
            InputEventKind::ButtonReleased(button) => {
                self.set_button(button, 0.0);
            }
            InputEventKind::ButtonChanged(button, value) => {
                self.set_button(button, value.clamp(0.0, 1.0));
            }
            InputEventKind::AxisChanged(axis, value) => {
                if axis != Axis::Unknown {
                    self.axes.insert(axis, value.clamp(-1.0, 1.0));
                }
            }
        }

        if let Some(previous) = self.last_event_at {
            self.intervals.push(IntervalSample {
                interval: at.saturating_duration_since(previous),
                at,
                kind: kind.event_kind(),
            });
        }
        self.last_event_at = Some(at);
        self.event_count += 1;
    }

    fn set_button(&mut self, button: Button, value: f32) {
        if button != Button::Unknown {
            self.buttons.insert(button, value);
        }
    }

    pub fn button_value(&self, button: Button) -> f32 {
        self.buttons.get(&button).copied().unwrap_or(0.0)
    }

    pub fn axis_value(&self, axis: Axis) -> f32 {
        self.axes.get(&axis).copied().unwrap_or(0.0)
    }

    pub fn intervals(&self) -> &IntervalWindow {
        &self.intervals
    }
}

/// Owned snapshot of the selected controller, safe to use while mutating the app.
pub struct ControllerView {
    pub id: GamepadId,
    pub info: ControllerInfo,
    pub connected: bool,
    pub buttons: Vec<(Button, f32)>,
    pub axes: Vec<(Axis, f32)>,
    pub samples: Vec<IntervalSample>,
    pub summary: Option<crate::stats::Summary>,
    pub event_count: u64,
    pub last_event_at: Option<Instant>,
}

pub struct ControllerListItem {
    pub id: GamepadId,
    pub name: String,
    pub connected: bool,
    pub power: String,
}

pub struct InputMonitor {
    receiver: Option<Receiver<InputMessage>>,
    status: BackendStatus,
    controllers: HashMap<GamepadId, Controller>,
    selected: Option<GamepadId>,
    presses: Vec<(GamepadId, Instant)>,
}

impl InputMonitor {
    pub fn new() -> Self {
        let mut monitor = Self {
            receiver: None,
            status: BackendStatus::Starting,
            controllers: HashMap::new(),
            selected: None,
            presses: Vec::new(),
        };
        monitor.spawn_backend();
        monitor
    }

    pub fn retry(&mut self) {
        self.receiver = None;
        self.controllers.clear();
        self.selected = None;
        self.presses.clear();
        self.status = BackendStatus::Starting;
        self.spawn_backend();
    }

    fn spawn_backend(&mut self) {
        let (sender, receiver) = mpsc::channel();
        match thread::Builder::new()
            .name("gamepad-input".to_owned())
            .spawn(move || input_thread(sender))
        {
            Ok(_) => self.receiver = Some(receiver),
            Err(error) => {
                self.status =
                    BackendStatus::Failed(format!("could not start input thread: {error}"))
            }
        }
    }

    pub fn status(&self) -> &BackendStatus {
        &self.status
    }

    pub fn poll(&mut self) {
        self.presses.clear();

        let mut messages = Vec::new();
        let mut disconnected = false;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(message) => messages.push(message),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
        }

        if disconnected {
            self.receiver = None;
            self.status = BackendStatus::Failed("the input thread stopped unexpectedly".to_owned());
            for controller in self.controllers.values_mut() {
                controller.connected = false;
            }
        }

        for message in messages {
            match message {
                InputMessage::Ready => {
                    if self.status == BackendStatus::Starting {
                        self.status = BackendStatus::Running;
                    }
                }
                InputMessage::Error(error) => {
                    self.status = BackendStatus::Failed(error);
                    for controller in self.controllers.values_mut() {
                        controller.connected = false;
                    }
                }
                InputMessage::Controller { id, info } => {
                    let controller = self
                        .controllers
                        .entry(id)
                        .or_insert_with(|| Controller::new(info.clone()));
                    controller.info = info;
                    controller.connected = true;
                    if self.selected.is_none() {
                        self.selected = Some(id);
                    }
                }
                InputMessage::Disconnected { id } => {
                    if let Some(controller) = self.controllers.get_mut(&id) {
                        controller.connected = false;
                    }
                }
                InputMessage::Event { id, at, kind } => {
                    let is_press = matches!(kind, InputEventKind::ButtonPressed(_));
                    let controller = self
                        .controllers
                        .entry(id)
                        .or_insert_with(|| Controller::new(ControllerInfo::unknown(id)));
                    controller.apply(kind, at);
                    if is_press {
                        self.presses.push((id, at));
                    }
                }
            }
        }
    }

    pub fn presses(&self) -> &[(GamepadId, Instant)] {
        &self.presses
    }

    pub fn select(&mut self, id: GamepadId) {
        if self.controllers.contains_key(&id) {
            self.selected = Some(id);
        }
    }

    pub fn selected_id(&self) -> Option<GamepadId> {
        self.selected
    }

    pub fn selected_controller(&self) -> Option<&Controller> {
        self.selected.and_then(|id| self.controllers.get(&id))
    }

    pub fn selected_view(&self) -> Option<ControllerView> {
        let id = self.selected?;
        let controller = self.controllers.get(&id)?;
        Some(ControllerView {
            id,
            info: controller.info.clone(),
            connected: controller.connected,
            buttons: BUTTON_ORDER
                .iter()
                .map(|button| (*button, controller.button_value(*button)))
                .collect(),
            axes: AXIS_ORDER
                .iter()
                .map(|axis| (*axis, controller.axis_value(*axis)))
                .collect(),
            samples: controller.intervals.iter().copied().collect(),
            summary: controller.intervals.summary(),
            event_count: controller.event_count,
            last_event_at: controller.last_event_at,
        })
    }

    pub fn controller_list(&self) -> Vec<ControllerListItem> {
        let mut list: Vec<ControllerListItem> = self
            .controllers
            .iter()
            .map(|(id, controller)| ControllerListItem {
                id: *id,
                name: controller.info.name.clone(),
                connected: controller.connected,
                power: controller.info.power.clone(),
            })
            .collect();
        list.sort_by_key(|item| usize::from(item.id));
        list
    }

    pub fn is_connected(&self, id: GamepadId) -> bool {
        self.controllers
            .get(&id)
            .map(|controller| controller.connected)
            .unwrap_or(false)
    }

    pub fn reset_selected_samples(&mut self) {
        if let Some(controller) = self.selected.and_then(|id| self.controllers.get_mut(&id)) {
            controller.intervals.clear();
            controller.last_event_at = None;
        }
    }
}

fn input_thread(sender: Sender<InputMessage>) {
    let mut gilrs = match Gilrs::new() {
        Ok(gilrs) => gilrs,
        Err(error) => {
            let _ = sender.send(InputMessage::Error(format!("{error}")));
            return;
        }
    };

    if sender.send(InputMessage::Ready).is_err() {
        return;
    }

    let mut known: HashMap<GamepadId, ControllerInfo> = HashMap::new();
    let mut last_refresh = Instant::now()
        .checked_sub(INFO_REFRESH_INTERVAL)
        .unwrap_or_else(Instant::now);
    let mut force_refresh = true;

    loop {
        while let Some(event) = gilrs.next_event() {
            let at = Instant::now();
            match event.event {
                EventType::Connected => {
                    force_refresh = true;
                }
                EventType::Disconnected => {
                    known.remove(&event.id);
                    if sender
                        .send(InputMessage::Disconnected { id: event.id })
                        .is_err()
                    {
                        return;
                    }
                }
                EventType::ButtonPressed(button, _) => {
                    if send_event(&sender, event.id, at, InputEventKind::ButtonPressed(button))
                        .is_err()
                    {
                        return;
                    }
                }
                EventType::ButtonReleased(button, _) => {
                    if send_event(
                        &sender,
                        event.id,
                        at,
                        InputEventKind::ButtonReleased(button),
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                EventType::ButtonChanged(button, value, _) => {
                    if send_event(
                        &sender,
                        event.id,
                        at,
                        InputEventKind::ButtonChanged(button, value),
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                EventType::AxisChanged(axis, value, _) => {
                    if send_event(
                        &sender,
                        event.id,
                        at,
                        InputEventKind::AxisChanged(axis, value),
                    )
                    .is_err()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }

        if force_refresh || last_refresh.elapsed() >= INFO_REFRESH_INTERVAL {
            force_refresh = false;
            last_refresh = Instant::now();

            let mut seen = HashSet::new();
            for (id, gamepad) in gilrs.gamepads() {
                seen.insert(id);
                let info = ControllerInfo::from_gamepad(&gamepad);
                if known.get(&id) != Some(&info) {
                    known.insert(id, info.clone());
                    if sender.send(InputMessage::Controller { id, info }).is_err() {
                        return;
                    }
                }
            }

            let removed: Vec<GamepadId> = known
                .keys()
                .copied()
                .filter(|id| !seen.contains(id))
                .collect();
            for id in removed {
                known.remove(&id);
                if sender.send(InputMessage::Disconnected { id }).is_err() {
                    return;
                }
            }
        }

        thread::sleep(INPUT_POLL_INTERVAL);
    }
}

fn send_event(
    sender: &Sender<InputMessage>,
    id: GamepadId,
    at: Instant,
    kind: InputEventKind,
) -> Result<(), mpsc::SendError<InputMessage>> {
    sender.send(InputMessage::Event { id, at, kind })
}

fn power_label(power: PowerInfo) -> String {
    match power {
        PowerInfo::Unknown => "Battery unknown".to_owned(),
        PowerInfo::Wired => "Wired / no battery".to_owned(),
        PowerInfo::Discharging(percent) => format!("Battery {percent}%"),
        PowerInfo::Charging(percent) => format!("Charging {percent}%"),
        PowerInfo::Charged => "Battery charged".to_owned(),
    }
}

fn mapping_label(source: MappingSource) -> &'static str {
    match source {
        MappingSource::SdlMappings => "SDL gamepad mapping",
        MappingSource::Driver => "Driver mapping",
        MappingSource::None => "No standard mapping",
    }
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    let hex: Vec<String> = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        hex[0..4].concat(),
        hex[4..6].concat(),
        hex[6..8].concat(),
        hex[8..10].concat(),
        hex[10..16].concat()
    )
}

/// Human-readable button name. Gilrs uses positional names so they work across
/// controller layouts; common Xbox/PlayStation equivalents are included too.
pub fn button_label(button: Button) -> &'static str {
    match button {
        Button::South => "South (A / Cross)",
        Button::East => "East (B / Circle)",
        Button::West => "West (X / Square)",
        Button::North => "North (Y / Triangle)",
        Button::LeftTrigger => "Left trigger 1 (LB / L1)",
        Button::RightTrigger => "Right trigger 1 (RB / R1)",
        Button::LeftTrigger2 => "Left trigger 2 (LT / L2)",
        Button::RightTrigger2 => "Right trigger 2 (RT / R2)",
        Button::Select => "Select / Back / Share",
        Button::Start => "Start / Options",
        Button::Mode => "Guide / Mode",
        Button::LeftThumb => "Left stick click (L3)",
        Button::RightThumb => "Right stick click (R3)",
        Button::DPadUp => "D-pad up",
        Button::DPadDown => "D-pad down",
        Button::DPadLeft => "D-pad left",
        Button::DPadRight => "D-pad right",
        Button::C => "C",
        Button::Z => "Z",
        Button::Unknown => "Unknown",
    }
}

pub fn axis_label(axis: Axis) -> &'static str {
    match axis {
        Axis::LeftStickX => "Left stick X",
        Axis::LeftStickY => "Left stick Y",
        Axis::RightStickX => "Right stick X",
        Axis::RightStickY => "Right stick Y",
        Axis::LeftZ => "Left trigger axis (Z)",
        Axis::RightZ => "Right trigger axis (Z)",
        Axis::DPadX => "D-pad X",
        Axis::DPadY => "D-pad Y",
        Axis::Unknown => "Unknown axis",
    }
}
