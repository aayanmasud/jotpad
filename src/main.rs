mod document;

use std::path::PathBuf;

use eframe::egui;
use rfd::FileDialog;

#[derive(Clone, Copy)]
enum PendingAction {
    New,
    Open,
}

struct JotPad {
    text: String,
    saved_text: String,
    path: Option<PathBuf>,
    pending_action: Option<PendingAction>,
    error: Option<String>,
    notice: Option<String>,
    dark_mode: bool,
}

impl Default for JotPad {
    fn default() -> Self {
        Self {
            text: String::new(),
            saved_text: String::new(),
            path: None,
            pending_action: None,
            error: None,
            notice: None,
            dark_mode: true,
        }
    }
}

impl JotPad {
    fn is_dirty(&self) -> bool {
        self.text != self.saved_text
    }

    fn request_action(&mut self, action: PendingAction) {
        if self.is_dirty() {
            self.pending_action = Some(action);
        } else {
            self.perform_action(action);
        }
    }

    fn perform_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => {
                self.text.clear();
                self.saved_text.clear();
                self.path = None;
                self.error = None;
                self.notice = None;
            }
            PendingAction::Open => self.open_file(),
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = FileDialog::new()
            .add_filter("Documents", &["txt", "md", "docx", "log"])
            .pick_file()
        {
            match document::load(&path) {
                Ok(contents) => {
                    self.text = contents;
                    self.saved_text = self.text.clone();
                    self.notice = if path
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("docx"))
                    {
                        Some("DOCX opened as text; formatting and page layout are not retained.".to_owned())
                    } else {
                        None
                    };
                    self.path = Some(path);
                    self.error = None;
                }
                Err(error) => self.error = Some(error),
            }
        }
    }

    fn save_file(&mut self, save_as: bool) {
        let path = if save_as || self.path.is_none() {
            FileDialog::new()
                .add_filter("Text documents and Word DOCX", &["txt", "md", "docx"])
                .set_file_name("untitled.txt")
                .save_file()
        } else {
            self.path.clone()
        };

        let Some(path) = path else {
            return;
        };

        match document::save(&path, &self.text) {
            Ok(()) => {
                self.saved_text = self.text.clone();
                self.path = Some(path);
                self.error = None;
                self.notice = None;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");

        format!(
            "{}{} — JotPad",
            name,
            if self.is_dirty() { " •" } else { "" }
        )
    }

    fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }
}

impl eframe::App for JotPad {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));

        let mut visuals = if self.dark_mode {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        };

        ctx.style_mut(|style| {
            style.visuals = visuals.clone();
            style.visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(12);
            style.visuals.widgets.active.corner_radius = egui::CornerRadius::same(12);
            style.visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(12);
            style.visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(12);
            style.visuals.selection.bg_fill = egui::Color32::from_rgb(86, 156, 214);
            style.visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(164, 206, 255));
        });

        visuals = ctx.style().visuals.clone();
        ctx.set_visuals(visuals);

        let (new, open, save) = ctx.input_mut(|input| {
            (
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
                input.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
            )
        });

        if new {
            self.request_action(PendingAction::New);
        }
        if open {
            self.request_action(PendingAction::Open);
        }
        if save {
            self.save_file(false);
        }

        egui::TopBottomPanel::top("toolbar")
            .min_height(56.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([94.0, 36.0], egui::Button::new("New").corner_radius(12.0))
                        .clicked()
                    {
                        self.request_action(PendingAction::New);
                    }
                    if ui
                        .add_sized([94.0, 36.0], egui::Button::new("Open").corner_radius(12.0))
                        .clicked()
                    {
                        self.request_action(PendingAction::Open);
                    }
                    if ui
                        .add_sized([94.0, 36.0], egui::Button::new("Save").corner_radius(12.0))
                        .clicked()
                    {
                        self.save_file(false);
                    }
                    if ui
                        .add_sized([110.0, 36.0], egui::Button::new("Save as…").corner_radius(12.0))
                        .clicked()
                    {
                        self.save_file(true);
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.toggle_value(&mut self.dark_mode, "Dark");
                    });
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let frame = egui::Frame::central_panel(&ctx.style())
                .fill(ui.visuals().panel_fill)
                .inner_margin(egui::Margin::same(8))
                .stroke(egui::Stroke::new(
                    1.0_f32,
                    ui.visuals().widgets.noninteractive.bg_stroke.color,
                ));

            frame.show(ui, |ui| {
                let text_edit = egui::TextEdit::multiline(&mut self.text)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(f32::INFINITY)
                    .desired_rows(28)
                    .lock_focus(true)
                    .code_editor();
                ui.add_sized(ui.available_size(), text_edit);
            });
        });

        egui::TopBottomPanel::bottom("status")
            .min_height(32.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(format!("{} words", self.word_count()));
                    ui.separator();
                    ui.label(format!("{} characters", self.text.chars().count()));
                    ui.separator();
                    ui.label(self.path.as_ref().map_or("Untitled", |path| {
                        path.file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("Untitled")
                    }));
                    if self.is_dirty() {
                        ui.separator();
                        ui.label("Unsaved changes");
                    }
                    if let Some(notice) = &self.notice {
                        ui.separator();
                        ui.label(notice);
                    }
                    if let Some(error) = &self.error {
                        ui.separator();
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                });
            });

        if let Some(action) = self.pending_action {
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.label("You have unsaved changes. What would you like to do?");
                        ui.horizontal(|ui| {
                            if ui
                                .add_sized([90.0, 32.0], egui::Button::new("Save").corner_radius(12.0))
                                .clicked()
                            {
                                self.save_file(false);
                                if !self.is_dirty() {
                                    self.pending_action = None;
                                    self.perform_action(action);
                                }
                            }
                            if ui
                                .add_sized([90.0, 32.0], egui::Button::new("Discard").corner_radius(12.0))
                                .clicked()
                            {
                                self.pending_action = None;
                                self.perform_action(action);
                            }
                            if ui
                                .add_sized([90.0, 32.0], egui::Button::new("Cancel").corner_radius(12.0))
                                .clicked()
                            {
                                self.pending_action = None;
                            }
                        });
                    });
                });
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([900.0, 650.0]),
        ..Default::default()
    };

    eframe::run_native(
        "JotPad",
        options,
        Box::new(|_creation_context| Ok(Box::<JotPad>::default())),
    )
}
