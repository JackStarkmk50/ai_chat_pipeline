use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use reply_voice::config::{self, Config, MAX_SPEED, MIN_SPEED};
use reply_voice::service::Service;
use reply_voice::status::Bridge;

struct Palette {
    bg: Color32,
    surf: Color32,
    tx: Color32,
    tx2: Color32,
    edge: Color32,
    line: Color32,
    or: Color32,
    orink: Color32,
    warn: Color32,
    warn_bg: Color32,
    ok: Color32,
    ok_bg: Color32,
}

fn hex(h: u32) -> Color32 {
    Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8)
}

fn palette(dark: bool) -> Palette {
    if dark {
        Palette {
            bg: hex(0x0B1220),
            surf: hex(0x131C2E),
            tx: hex(0xEAF0FA),
            tx2: hex(0xA8B4CC),
            edge: hex(0x66738F),
            line: hex(0x1F2A40),
            or: hex(0xFF7A2B),
            orink: hex(0x1A0D02),
            warn: hex(0xF2C14E),
            warn_bg: hex(0x33290F),
            ok: hex(0x5CC48A),
            ok_bg: hex(0x16291E),
        }
    } else {
        Palette {
            bg: hex(0xF4F7FB),
            surf: hex(0xFFFFFF),
            tx: hex(0x0B1424),
            tx2: hex(0x44506A),
            edge: hex(0x78849C),
            line: hex(0xDAE1EC),
            or: hex(0xC2410C),
            orink: hex(0xFFFFFF),
            warn: hex(0x7A5200),
            warn_bg: hex(0xFBF0D3),
            ok: hex(0x1F6B40),
            ok_bg: hex(0xE3F3E9),
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    FirstRun(u8),
    Settings,
}

struct App {
    path: std::path::PathBuf,
    saved: Config,
    draft: Config,
    screen: Screen,
    svc: Service,
    error: Option<String>,
    sample_note: Option<String>,
    tray: bool,
}

pub fn run() {
    let path = config::default_path();
    let saved = Config::load(&path);
    let mut svc = Service::new();
    svc.apply(&saved);
    let screen = if saved.first_run_done {
        Screen::Settings
    } else {
        Screen::FirstRun(1)
    };
    let app = App {
        path,
        draft: saved.clone(),
        saved,
        screen,
        svc,
        error: None,
        sample_note: None,
        tray: false,
    };
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([440.0, 780.0])
            .with_min_inner_size([400.0, 520.0])
            .with_title("Reply Voice"),
        ..Default::default()
    };
    let _ = eframe::run_native(
        "Reply Voice",
        opts,
        Box::new(|cc| {
            let mut app = app;
            app.tray = tray::start(&cc.egui_ctx);
            Ok(Box::new(app))
        }),
    );
}

fn apply_style(ctx: &egui::Context, p: &Palette, dark: bool) {
    let mut v = if dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    };
    v.panel_fill = p.bg;
    v.window_fill = p.surf;
    v.extreme_bg_color = p.surf;
    v.override_text_color = Some(p.tx);
    v.selection.bg_fill = p.or;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
    for w in [
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.bg_stroke = Stroke::new(1.0, p.edge);
        w.corner_radius = CornerRadius::same(10);
    }
    v.widgets.inactive.weak_bg_fill = p.surf;
    v.widgets.hovered.weak_bg_fill = p.line;
    v.widgets.active.weak_bg_fill = p.line;
    v.widgets.noninteractive.corner_radius = CornerRadius::same(10);
    v.text_cursor.stroke = Stroke::new(2.0, p.or);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = Vec2::new(10.0, 10.0);
        s.spacing.button_padding = Vec2::new(16.0, 9.0);
        s.spacing.interact_size.y = 36.0;
    });
}

fn primary(ui: &mut egui::Ui, p: &Palette, text: &str, enabled: bool) -> egui::Response {
    let (fill, ink) = if enabled {
        (p.or, p.orink)
    } else {
        (p.line, p.tx2)
    };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(ink).strong())
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(20))
            .min_size(Vec2::new(120.0, 38.0)),
    )
}

fn plain(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(text)
            .corner_radius(CornerRadius::same(20))
            .min_size(Vec2::new(90.0, 38.0)),
    )
}

fn pill(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    ui.scope(|ui| {
        ui.spacing_mut().interact_size.y = 0.0;
        egui::Frame::new()
            .fill(bg)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(egui::Margin::symmetric(10, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(text).color(fg).size(12.5));
            });
    });
}

fn card(ui: &mut egui::Ui, fill: Color32, stroke: Color32, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::new()
        .fill(fill)
        .stroke(Stroke::new(1.0, stroke))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

fn bridge_pill(ui: &mut egui::Ui, p: &Palette, svc: &Service) {
    match svc.shared.get().bridge {
        Bridge::NotConnected => pill(ui, "Bridge not connected", p.warn, p.warn_bg),
        Bridge::Listening => pill(ui, "Waiting for a reply", p.warn, p.warn_bg),
        Bridge::Connected => pill(ui, "Reply received", p.ok, p.ok_bg),
        Bridge::Failed(e) => pill(ui, &format!("Link problem: {e}"), p.warn, p.warn_bg),
    }
}

fn header(ui: &mut egui::Ui, _p: &Palette, title: &str) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(15.0).strong());
    });
}

impl App {
    fn voice_block(&mut self, ui: &mut egui::Ui, p: &Palette, in_wizard: bool) {
        use reply_voice::service::VoiceKind;
        use reply_voice::voice::Download;
        if in_wizard {
            ui.label(RichText::new("Voice").color(p.tx2));
        }
        let label = match self.svc.voice {
            VoiceKind::Piper => &self.svc.voice_name,
            VoiceKind::System => "System voice (fallback)",
            VoiceKind::None => &self.draft.voice,
        };
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut label.to_string()).desired_width(f32::INFINITY),
        );
        match self.svc.installer.get() {
            Download::Downloading { done, total } => {
                card(ui, p.surf, p.line, |ui| {
                    let frac = if total == 0 {
                        0.0
                    } else {
                        done as f32 / total as f32
                    };
                    ui.horizontal(|ui| {
                        ui.label("Downloading");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                RichText::new(format!("{}%", (frac * 100.0) as u32)).color(p.tx2),
                            );
                        });
                    });
                    ui.add(egui::ProgressBar::new(frac).fill(p.or).desired_height(8.0));
                    ui.label(
                        RichText::new(format!("{} of {} MB", done / 1_000_000, total / 1_000_000))
                            .color(p.tx2)
                            .size(12.0),
                    );
                });
                if plain(ui, "Cancel").clicked() {
                    self.svc.installer.cancel();
                }
                return;
            }
            Download::Failed(msg) => {
                card(ui, p.warn_bg, p.warn, |ui| {
                    ui.label(
                        RichText::new("The voice could not be downloaded")
                            .color(p.warn)
                            .strong(),
                    );
                    ui.label(RichText::new(msg).color(p.warn));
                });
            }
            _ => {}
        }
        if self.svc.voice == VoiceKind::Piper {
            ui.horizontal(|ui| {
                pill(ui, "Ready", p.ok, p.ok_bg);
                ui.label(RichText::new("On this computer").color(p.tx2).size(12.5));
            });
            return;
        }
        let failed = matches!(self.svc.installer.get(), Download::Failed(_));
        let system = self.svc.voice == VoiceKind::System;
        let mb = self
            .svc
            .manifest
            .as_ref()
            .map(|m| m.total_bytes() / 1_000_000);
        if !failed {
            card(ui, p.warn_bg, p.warn, |ui| {
                let title = match (system, mb.is_some()) {
                    (true, _) => "Using the voice that came with this computer",
                    (false, true) => "Voice not on this computer yet",
                    (false, false) => "No voice yet",
                };
                ui.label(RichText::new(title).color(p.warn).strong());
                let body = match (system, mb) {
                    (true, Some(mb)) => format!("Download the Reply Voice voice (about {mb} MB, once) for a more natural sound."),
                    (true, None) => "No downloadable voice is set up. This voice is the fallback.".to_string(),
                    (false, Some(mb)) => format!("Download it once (about {mb} MB). Until then nothing can be read aloud."),
                    (false, None) => "No downloadable voice is set up and this computer has none. Nothing can be read aloud yet.".to_string(),
                };
                ui.label(RichText::new(body).color(p.warn));
            });
        }
        if in_wizard || failed || system {
            ui.horizontal(|ui| {
                let text = if failed {
                    "Try again"
                } else {
                    "Download voice"
                };
                if primary(ui, p, text, mb.is_some()).clicked() {
                    if let Some(m) = self.svc.manifest.clone() {
                        self.svc.installer.start(self.svc.base.clone(), m);
                    }
                }
                if in_wizard && self.svc.voice == VoiceKind::None {
                    primary(ui, p, "Play a sample", false);
                }
            });
        }
    }

    fn speed_block(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Speed").color(p.tx2));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{:.2}x", self.draft.speed)).color(p.tx2))
            });
        });
        ui.spacing_mut().slider_width = ui.available_width() - 4.0;
        ui.add(egui::Slider::new(&mut self.draft.speed, MIN_SPEED..=MAX_SPEED).show_value(false));
    }

    fn sample_button(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let enabled = self.svc.voice_ready();
        if primary(ui, p, "Play a sample", enabled).clicked() {
            let speed = self.draft.speed;
            match reply_voice::service::open_engine(&self.svc.base).1 {
                Some(mut v) => {
                    let shared = self.svc.shared.clone();
                    std::thread::spawn(move || {
                        let r = v.speak("This is how replies will sound.", speed);
                        shared.update(|s| s.last_error = r.err());
                    });
                    self.sample_note = None;
                }
                None => self.sample_note = Some("Could not start the voice".into()),
            }
        }
        if let Some(e) = self.svc.shared.get().last_error {
            ui.label(RichText::new(format!("Sample problem: {e}")).color(p.warn));
        }
        if let Some(n) = &self.sample_note {
            ui.label(RichText::new(n).color(p.warn));
        }
    }

    fn link_block(&mut self, ui: &mut egui::Ui, p: &Palette) {
        ui.label(RichText::new("Connection address").color(p.tx2));
        ui.add(
            egui::TextEdit::singleline(&mut self.draft.address)
                .hint_text("127.0.0.1:47615")
                .desired_width(f32::INFINITY)
                .margin(Vec2::new(10.0, 8.0)),
        );
        ui.label(RichText::new("Access token, kept separate").color(p.tx2));
        ui.horizontal(|ui| {
            let w = ui.available_width() - 100.0;
            ui.add(
                egui::TextEdit::singleline(&mut self.draft.token)
                    .password(true)
                    .hint_text("Not set")
                    .desired_width(w)
                    .margin(Vec2::new(10.0, 8.0)),
            );
            if plain(ui, "Make one").clicked() {
                self.draft.token = config::generate_token();
            }
        });
        ui.label(
            RichText::new("The token is hidden. Anything that sends replies here must present it.")
                .color(p.tx2)
                .size(12.0),
        );
        bridge_pill(ui, p, &self.svc);
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(p.warn));
        }
    }

    fn save(&mut self) -> bool {
        self.draft.clamp();
        if let Err(e) = self.draft.validate() {
            self.error = Some(e.to_string());
            return false;
        }
        if let Err(e) = self.draft.save(&self.path) {
            self.error = Some(format!("Could not save: {e}"));
            return false;
        }
        if self.draft.start_with_computer != self.saved.start_with_computer {
            if let Err(e) = reply_voice::autostart::set(self.draft.start_with_computer) {
                self.error = Some(e);
                return false;
            }
        }
        self.error = None;
        self.saved = self.draft.clone();
        self.svc.apply(&self.saved);
        true
    }

    fn first_run(&mut self, ui: &mut egui::Ui, p: &Palette, step: u8) {
        header(ui, p, "Reply Voice");
        ui.horizontal(|ui| {
            for i in 1..=3u8 {
                let (r, _) = ui.allocate_exact_size(
                    Vec2::new((400.0 - 20.0) / 3.0, 4.0),
                    egui::Sense::hover(),
                );
                ui.painter()
                    .rect_filled(r, 2.0, if i <= step { p.or } else { p.line });
            }
        });
        ui.label(
            RichText::new(format!("Step {step} of 3"))
                .color(p.tx2)
                .size(12.5),
        );
        ui.add_space(8.0);
        match step {
            1 => {
                let ready = self.svc.voice_ready();
                ui.label(
                    RichText::new(if ready {
                        "Voice is ready"
                    } else {
                        "Get the voice"
                    })
                    .size(26.0)
                    .strong(),
                );
                ui.label(RichText::new("Reply Voice reads each reply aloud with a voice that runs on this computer. English first.").color(p.tx2));
                self.voice_block(ui, p, true);
                if ready {
                    self.sample_button(ui, p);
                }
                ui.add_space(ui.available_height() - 52.0);
                ui.horizontal(|ui| {
                    if plain(ui, "Skip for now").clicked() {
                        self.screen = Screen::FirstRun(2);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, p, "Next", ready).clicked() {
                            self.screen = Screen::FirstRun(2);
                        }
                    });
                });
            }
            2 => {
                ui.label(RichText::new("Link to your replies").size(26.0).strong());
                ui.label(
                    RichText::new(
                        "Set where replies arrive and the token that proves they are yours.",
                    )
                    .color(p.tx2),
                );
                self.link_block(ui, p);
                ui.add_space((ui.available_height() - 52.0).max(0.0));
                ui.horizontal(|ui| {
                    if plain(ui, "Back").clicked() {
                        self.screen = Screen::FirstRun(1);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, p, "Next", true).clicked() {
                            self.draft.clamp();
                            match self.draft.validate() {
                                Ok(()) => {
                                    self.error = None;
                                    self.screen = Screen::FirstRun(3);
                                }
                                Err(e) => self.error = Some(e.to_string()),
                            }
                        }
                        if plain(ui, "Skip for now").clicked() {
                            self.error = None;
                            self.screen = Screen::FirstRun(3);
                        }
                    });
                });
            }
            _ => {
                let voice = self.svc.voice_ready();
                let link = self.draft.link_configured();
                let all = voice && link;
                ui.label(
                    RichText::new(if all {
                        "Ready to listen"
                    } else {
                        "Almost ready"
                    })
                    .size(26.0)
                    .strong(),
                );
                ui.label(RichText::new(if all { "Replies sent to this app will be read aloud. Nothing has arrived yet." } else { "Two things are still open. The app will not read anything until both are done." }).color(p.tx2));
                card(ui, p.surf, p.line, |ui| {
                    for (name, done) in [("Voice", voice), ("Link to your replies", link)] {
                        ui.horizontal(|ui| {
                            ui.label(name);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if done {
                                        pill(ui, "Done", p.ok, p.ok_bg)
                                    } else {
                                        pill(ui, "To do", p.warn, p.warn_bg)
                                    }
                                },
                            );
                        });
                    }
                });
                ui.checkbox(
                    &mut self.draft.start_with_computer,
                    "Start with the computer (off until you turn it on)",
                );
                bridge_pill(ui, p, &self.svc);
                if let Some(e) = &self.error {
                    ui.label(RichText::new(e).color(p.warn));
                }
                ui.add_space((ui.available_height() - 52.0).max(0.0));
                ui.horizontal(|ui| {
                    if plain(ui, "Back").clicked() {
                        self.screen = Screen::FirstRun(2);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if primary(ui, p, if all { "Done" } else { "Finish setup" }, true).clicked()
                        {
                            self.draft.first_run_done = true;
                            if self.save() {
                                self.screen = Screen::Settings;
                            }
                        }
                    });
                });
            }
        }
    }

    fn settings(&mut self, ui: &mut egui::Ui, p: &Palette) {
        header(ui, p, "Settings");
        ui.separator();
        ui.label(RichText::new("Voice").strong());
        self.voice_block(ui, p, false);
        if self.svc.voice_ready() {
            self.sample_button(ui, p);
        }
        self.speed_block(ui, p);
        ui.separator();
        ui.label(RichText::new("Startup").strong());
        ui.checkbox(
            &mut self.draft.start_with_computer,
            "Start with the computer",
        );
        ui.separator();
        ui.label(RichText::new("Link to your replies").strong());
        self.link_block(ui, p);
        ui.add_space((ui.available_height() - 52.0).max(0.0));
        ui.horizontal(|ui| {
            if plain(ui, "Close").clicked() {
                self.draft = self.saved.clone();
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, p, "Save", self.draft != self.saved).clicked() {
                    self.save();
                }
            });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let ctx = &ctx;
        let dark = ctx.global_style().visuals.dark_mode;
        let p = palette(dark);
        apply_style(ctx, &p, dark);
        if self.svc.installer.get() == reply_voice::voice::Download::Done {
            *self.svc.installer.state.lock().unwrap() = reply_voice::voice::Download::Idle;
            self.svc.refresh_voice();
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(500));
        // With a tray icon, closing the window keeps the app running in the background.
        if self.tray && ctx.input(|i| i.viewport().close_requested()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.bg)
                    .inner_margin(egui::Margin::same(20)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.set_max_width(ui.available_width() - 14.0);
                        match self.screen {
                            Screen::FirstRun(s) => self.first_run(ui, &p, s),
                            Screen::Settings => self.settings(ui, &p),
                        }
                    });
            });
    }
}

#[cfg(windows)]
mod tray {
    use eframe::egui::{self, ViewportCommand};
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIconBuilder};

    pub fn start(ctx: &egui::Context) -> bool {
        let open = MenuItem::with_id("open", "Open Reply Voice", true, None);
        let quit = MenuItem::with_id("quit", "Quit", true, None);
        let menu = Menu::new();
        if menu.append(&open).is_err() || menu.append(&quit).is_err() {
            return false;
        }
        let mut rgba = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32i32 {
            for x in 0..32i32 {
                let (dx, dy) = (x - 16, y - 16);
                let inside = dx * dx + dy * dy <= 15 * 15;
                rgba.extend_from_slice(if inside {
                    &[0xC2, 0x41, 0x0C, 0xFF]
                } else {
                    &[0, 0, 0, 0]
                });
            }
        }
        let Ok(icon) = Icon::from_rgba(rgba, 32, 32) else {
            return false;
        };
        let Ok(tray) = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Reply Voice")
            .with_icon(icon)
            .build()
        else {
            return false;
        };
        // The icon must live as long as the app.
        Box::leak(Box::new(tray));
        let c = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if e.id.0 == "open" {
                c.send_viewport_cmd(ViewportCommand::Visible(true));
                c.send_viewport_cmd(ViewportCommand::Focus);
            } else if e.id.0 == "quit" {
                std::process::exit(0);
            }
        }));
        true
    }
}

#[cfg(not(windows))]
mod tray {
    use eframe::egui;
    pub fn start(_ctx: &egui::Context) -> bool {
        false
    }
}
