mod document;

use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

use eframe::egui;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use rfd::FileDialog;

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/aayanmasud/jotpad/releases/latest";
const MAX_RECENT_FILES: usize = 10;

#[derive(Clone, Copy)]
enum PendingAction { New, Open }
#[derive(Clone, Copy)]
enum FormatAction { Bold, Italic, Code, Heading(u8), Paragraph, BulletList, NumberedList, Quote, CodeBlock, HorizontalRule }
#[derive(Clone, Copy)]
enum PreviewKind { Paragraph, Heading(HeadingLevel), ListItem, Code, Quote }
struct PreviewBlock { kind: PreviewKind, text: String }
struct ReleaseInfo { version: String, url: String }
enum UpdateCheckResult { Update(ReleaseInfo), UpToDate, NoRelease, Failed(String) }

struct JotPad {
    text: String,
    saved_text: String,
    path: Option<PathBuf>,
    recent_files: Vec<PathBuf>,
    show_start: bool,
    pending_action: Option<PendingAction>,
    pending_format: Option<FormatAction>,
    selection: (usize, usize),
    error: Option<String>,
    notice: Option<String>,
    dark_mode: bool,
    show_preview: bool,
    editor_font_size: f32,
    editor_color: egui::Color32,
    update_request: Sender<()>,
    update_results: Receiver<UpdateCheckResult>,
    update_available: Option<ReleaseInfo>,
    update_status: String,
}

fn recent_file_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/jotpad/recent-files"))
}

fn load_recent_files() -> Vec<PathBuf> {
    let Some(path) = recent_file_path() else { return Vec::new() };
    std::fs::read_to_string(path).unwrap_or_default().lines().map(PathBuf::from)
        .filter(|path| path.exists()).take(MAX_RECENT_FILES).collect()
}

impl Default for JotPad {
    fn default() -> Self {
        let (request_tx, request_rx) = mpsc::channel::<()>();
        let (result_tx, result_rx) = mpsc::channel::<UpdateCheckResult>();
        thread::spawn(move || {
            while request_rx.recv().is_ok() {
                if result_tx.send(check_for_updates()).is_err() { break; }
            }
        });
        let _ = request_tx.send(());
        Self {
            text: String::new(), saved_text: String::new(), path: None,
            recent_files: load_recent_files(), show_start: true,
            pending_action: None, pending_format: None, selection: (0, 0),
            error: None, notice: None, dark_mode: true, show_preview: false,
            editor_font_size: 16.0, editor_color: egui::Color32::WHITE,
            update_request: request_tx, update_results: result_rx,
            update_available: None, update_status: "Checking for updates…".to_owned(),
        }
    }
}

fn check_for_updates() -> UpdateCheckResult {
    let client = match reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).build() {
        Ok(client) => client,
        Err(error) => return UpdateCheckResult::Failed(error.to_string()),
    };
    let response = match client.get(LATEST_RELEASE_API)
        .header(reqwest::header::USER_AGENT, concat!("JotPad/", env!("CARGO_PKG_VERSION"))).send() {
        Ok(response) => response,
        Err(error) => return UpdateCheckResult::Failed(error.to_string()),
    };
    if response.status() == reqwest::StatusCode::NOT_FOUND { return UpdateCheckResult::NoRelease; }
    if !response.status().is_success() { return UpdateCheckResult::Failed(format!("GitHub returned HTTP {}", response.status())); }
    let release: serde_json::Value = match response.json() {
        Ok(value) => value,
        Err(error) => return UpdateCheckResult::Failed(format!("Could not read release information: {error}")),
    };
    let Some(tag) = release.get("tag_name").and_then(serde_json::Value::as_str) else {
        return UpdateCheckResult::Failed("GitHub release is missing its version tag".to_owned());
    };
    let url = release.get("html_url").and_then(serde_json::Value::as_str)
        .unwrap_or("https://github.com/aayanmasud/jotpad/releases");
    let Ok(current) = semver::Version::parse(env!("CARGO_PKG_VERSION")) else {
        return UpdateCheckResult::Failed("The installed version is invalid".to_owned());
    };
    let Ok(latest) = semver::Version::parse(tag.trim_start_matches('v')) else {
        return UpdateCheckResult::Failed(format!("Release tag '{tag}' is not a semantic version"));
    };
    if latest > current {
        UpdateCheckResult::Update(ReleaseInfo { version: tag.to_owned(), url: url.to_owned() })
    } else { UpdateCheckResult::UpToDate }
}

impl JotPad {
    fn is_dirty(&self) -> bool { self.text != self.saved_text }

    fn remember_file(&mut self, path: &PathBuf) {
        self.recent_files.retain(|recent| recent != path);
        self.recent_files.insert(0, path.clone());
        self.recent_files.truncate(MAX_RECENT_FILES);
        if let Some(storage) = recent_file_path() {
            if let Some(parent) = storage.parent() { let _ = std::fs::create_dir_all(parent); }
            let contents = self.recent_files.iter().map(|p| p.to_string_lossy()).collect::<Vec<_>>().join("\n");
            let _ = std::fs::write(storage, contents);
        }
    }

    fn request_action(&mut self, action: PendingAction) {
        if self.is_dirty() { self.pending_action = Some(action); } else { self.perform_action(action); }
    }

    fn perform_action(&mut self, action: PendingAction) {
        match action {
            PendingAction::New => {
                self.text.clear(); self.saved_text.clear(); self.path = None;
                self.error = None; self.notice = None; self.selection = (0, 0); self.show_start = false;
            }
            PendingAction::Open => self.open_file(),
        }
    }

    fn load_path(&mut self, path: PathBuf) {
        match document::load(&path) {
            Ok(contents) => {
                self.text = contents; self.saved_text = self.text.clone();
                self.notice = if path.extension().and_then(|ext| ext.to_str()).is_some_and(|ext| ext.eq_ignore_ascii_case("docx")) {
                    Some("DOCX opened as text; formatting and page layout are not retained.".to_owned())
                } else { None };
                self.path = Some(path.clone()); self.remember_file(&path); self.error = None;
                self.selection = (0, 0); self.show_start = false;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn open_file(&mut self) {
        if let Some(path) = FileDialog::new().add_filter("Documents", &["txt", "md", "docx", "log"]).pick_file() { self.load_path(path); }
    }

    fn save_file(&mut self, save_as: bool) {
        let path = if save_as || self.path.is_none() {
            FileDialog::new().add_filter("Text documents and Markdown", &["txt", "md"]).set_file_name("untitled.md").save_file()
        } else { self.path.clone() };
        let Some(path) = path else { return };
        match document::save(&path, &self.text) {
            Ok(()) => {
                self.saved_text = self.text.clone(); self.path = Some(path.clone()); self.remember_file(&path);
                self.error = None; self.notice = None; self.show_start = false;
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn export_docx(&mut self) {
        let path = FileDialog::new().add_filter("Word document", &["docx"]).set_file_name("untitled.docx").save_file();
        let Some(path) = path else { return };
        match document::save(&path, &markdown_to_plain_text(&self.text)) {
            Ok(()) => { self.error = None; self.notice = Some("Exported to DOCX as plain text; Markdown styling is not included.".to_owned()); }
            Err(error) => self.error = Some(error),
        }
    }

    fn apply_format(&mut self, action: FormatAction) {
        let (_, end) = self.selection;
        match action {
            FormatAction::Bold => self.wrap_selection("**", "**"),
            FormatAction::Italic => self.wrap_selection("*", "*"),
            FormatAction::Code => self.wrap_selection("`", "`"),
            FormatAction::Heading(level) => self.prefix_selected_lines(&format!("{} ", "#".repeat(level as usize))),
            FormatAction::Paragraph => self.prefix_selected_lines(""),
            FormatAction::BulletList => self.prefix_selected_lines("- "),
            FormatAction::NumberedList => self.prefix_selected_lines("1. "),
            FormatAction::Quote => self.prefix_selected_lines("> "),
            FormatAction::CodeBlock => self.wrap_selection("```\n", "\n```"),
            FormatAction::HorizontalRule => { let index = char_to_byte(&self.text, end); self.text.insert_str(index, "\n\n---\n\n"); }
        }
    }

    fn wrap_selection(&mut self, before: &str, after: &str) {
        let (start, end) = self.selection;
        let start = char_to_byte(&self.text, start); let end = char_to_byte(&self.text, end);
        self.text.insert_str(end, after); self.text.insert_str(start, before);
    }

    fn prefix_selected_lines(&mut self, prefix: &str) {
        let (start, end) = self.selection;
        let start = char_to_byte(&self.text, start); let end = char_to_byte(&self.text, end);
        let line_start = self.text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = self.text[end..].find('\n').map_or(self.text.len(), |i| end + i);
        if prefix.is_empty() {
            let line = &self.text[line_start..line_end];
            let content_start = line.find(|c: char| c != '#' && c != ' ').unwrap_or(line.len());
            let content = line[content_start..].to_owned(); self.text.replace_range(line_start..line_end, &content);
        } else {
            let selected = self.text[line_start..line_end].to_owned();
            let formatted = selected.lines().map(|line| format!("{prefix}{line}")).collect::<Vec<_>>().join("\n");
            self.text.replace_range(line_start..line_end, &formatted);
        }
    }

    fn title(&self) -> String {
        let name = self.path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or("Untitled");
        format!("{}{} — JotPad", name, if self.is_dirty() { " •" } else { "" })
    }
    fn word_count(&self) -> usize { self.text.split_whitespace().count() }
}

fn char_to_byte(text: &str, index: usize) -> usize { text.char_indices().nth(index).map_or(text.len(), |(byte, _)| byte) }
fn heading_size(level: HeadingLevel) -> f32 {
    match level { HeadingLevel::H1 => 26.0, HeadingLevel::H2 => 23.0, HeadingLevel::H3 => 20.0, HeadingLevel::H4 => 18.0, HeadingLevel::H5 => 16.0, HeadingLevel::H6 => 14.0 }
}

fn markdown_preview_blocks(markdown: &str) -> Vec<PreviewBlock> {
    let mut blocks = Vec::new(); let mut current: Option<PreviewBlock> = None;
    let flush = |current: &mut Option<PreviewBlock>, blocks: &mut Vec<PreviewBlock>| {
        if let Some(block) = current.take() { if !block.text.trim().is_empty() { blocks.push(block); } }
    };
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => { flush(&mut current, &mut blocks); current = Some(PreviewBlock { kind: PreviewKind::Heading(level), text: String::new() }); }
            Event::Start(Tag::Paragraph) => { flush(&mut current, &mut blocks); current = Some(PreviewBlock { kind: PreviewKind::Paragraph, text: String::new() }); }
            Event::Start(Tag::Item) => { flush(&mut current, &mut blocks); current = Some(PreviewBlock { kind: PreviewKind::ListItem, text: "• ".to_owned() }); }
            Event::Start(Tag::CodeBlock(_)) => { flush(&mut current, &mut blocks); current = Some(PreviewBlock { kind: PreviewKind::Code, text: String::new() }); }
            Event::Start(Tag::BlockQuote(_)) => { flush(&mut current, &mut blocks); current = Some(PreviewBlock { kind: PreviewKind::Quote, text: "│ ".to_owned() }); }
            Event::End(TagEnd::Heading(_) | TagEnd::Paragraph | TagEnd::Item | TagEnd::CodeBlock | TagEnd::BlockQuote) => flush(&mut current, &mut blocks),
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => current.get_or_insert_with(|| PreviewBlock { kind: PreviewKind::Paragraph, text: String::new() }).text.push_str(&text),
            Event::SoftBreak | Event::HardBreak => if let Some(block) = current.as_mut() { block.text.push('\n'); },
            Event::Rule => { flush(&mut current, &mut blocks); blocks.push(PreviewBlock { kind: PreviewKind::Paragraph, text: "────────────────────────".to_owned() }); }
            Event::TaskListMarker(checked) => current.get_or_insert_with(|| PreviewBlock { kind: PreviewKind::ListItem, text: String::new() }).text.push_str(if checked { "☑ " } else { "☐ " }),
            _ => {}
        }
    }
    flush(&mut current, &mut blocks); blocks
}

fn markdown_to_plain_text(markdown: &str) -> String {
    let mut output = String::new();
    for event in Parser::new(markdown) {
        match event {
            Event::Start(Tag::Item) => output.push_str("• "),
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text) => output.push_str(&text),
            Event::SoftBreak | Event::HardBreak => output.push('\n'),
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_) | TagEnd::Item | TagEnd::CodeBlock | TagEnd::BlockQuote) => output.push('\n'),
            Event::Rule => output.push_str("\n────────────────────────\n"),
            Event::TaskListMarker(checked) => output.push_str(if checked { "☑ " } else { "☐ " }),
            _ => {}
        }
    }
    output.trim().to_owned()
}

impl eframe::App for JotPad {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.title()));
        ctx.set_visuals(if self.dark_mode { egui::Visuals::dark() } else { egui::Visuals::light() });
        for result in self.update_results.try_iter() {
            match result {
                UpdateCheckResult::Update(release) => { self.update_status = format!("Version {} is available", release.version); self.update_available = Some(release); }
                UpdateCheckResult::UpToDate => { self.update_status = "JotPad is up to date".to_owned(); self.update_available = None; }
                UpdateCheckResult::NoRelease => { self.update_status = "No published GitHub releases yet".to_owned(); self.update_available = None; }
                UpdateCheckResult::Failed(error) => self.update_status = format!("Update check failed: {error}"),
            }
        }
        if self.update_status == "Checking for updates…" { ctx.request_repaint_after(Duration::from_millis(250)); }
        if let Some(format) = self.pending_format.take() { self.apply_format(format); }
        let (new, open, save) = ctx.input_mut(|input| (
            input.consume_key(egui::Modifiers::COMMAND, egui::Key::N),
            input.consume_key(egui::Modifiers::COMMAND, egui::Key::O),
            input.consume_key(egui::Modifiers::COMMAND, egui::Key::S),
        ));
        if new { self.request_action(PendingAction::New); }
        if open { self.request_action(PendingAction::Open); }
        if save { self.save_file(false); }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(4.0, 3.0);
            ui.spacing_mut().button_padding = egui::vec2(5.0, 3.0);
            if let Some(release) = &self.update_available {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("JotPad {} is available.", release.version)).strong());
                    ui.hyperlink_to("View release", &release.url);
                    if ui.small_button("Dismiss").clicked() { self.update_available = None; }
                });
                ui.separator();
            }
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("New").clicked() { self.request_action(PendingAction::New); }
                if ui.small_button("Open").clicked() { self.request_action(PendingAction::Open); }
                if ui.small_button("Save").clicked() { self.save_file(false); }
                if ui.small_button("Save as…").clicked() { self.save_file(true); }
                if ui.small_button("DOCX").clicked() { self.export_docx(); }
                ui.separator();
                if ui.small_button(egui::RichText::new("B").strong()).on_hover_text("Bold").clicked() { self.pending_format = Some(FormatAction::Bold); }
                if ui.small_button(egui::RichText::new("I").italics()).on_hover_text("Italic").clicked() { self.pending_format = Some(FormatAction::Italic); }
                if ui.small_button("Code").clicked() { self.pending_format = Some(FormatAction::Code); }
                ui.menu_button("H", |ui| for level in 1..=6 { if ui.button(format!("Heading {level}")).clicked() { self.pending_format = Some(FormatAction::Heading(level)); ui.close_menu(); } });
                ui.menu_button("¶", |ui| {
                    if ui.button("Paragraph").clicked() { self.pending_format = Some(FormatAction::Paragraph); ui.close_menu(); }
                    if ui.button("Bullet list").clicked() { self.pending_format = Some(FormatAction::BulletList); ui.close_menu(); }
                    if ui.button("Numbered list").clicked() { self.pending_format = Some(FormatAction::NumberedList); ui.close_menu(); }
                    if ui.button("Block quote").clicked() { self.pending_format = Some(FormatAction::Quote); ui.close_menu(); }
                    if ui.button("Code block").clicked() { self.pending_format = Some(FormatAction::CodeBlock); ui.close_menu(); }
                    if ui.button("Horizontal rule").clicked() { self.pending_format = Some(FormatAction::HorizontalRule); ui.close_menu(); }
                });
                ui.label("Size"); ui.add(egui::Slider::new(&mut self.editor_font_size, 10.0..=32.0).show_value(false).fixed_decimals(0).max_width(70.0));
                ui.color_edit_button_srgba(&mut self.editor_color).on_hover_text("Editor text color");
                ui.checkbox(&mut self.show_preview, "Preview"); ui.toggle_value(&mut self.dark_mode, "Dark");
                if ui.small_button("Updates").on_hover_text(&self.update_status).clicked() { self.update_status = "Checking for updates…".to_owned(); let _ = self.update_request.send(()); }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_start {
                ui.vertical_centered(|ui| {
                    ui.add_space(ui.available_height() * 0.12);
                    ui.heading("Welcome to JotPad");
                    ui.label("A simple space for your notes and Markdown documents.");
                    ui.add_space(18.0);
                    ui.horizontal(|ui| {
                        if ui.add_sized([150.0, 36.0], egui::Button::new("＋  Create new")).clicked() { self.request_action(PendingAction::New); }
                        if ui.add_sized([150.0, 36.0], egui::Button::new("Open a file")).clicked() { self.request_action(PendingAction::Open); }
                    });
                    ui.add_space(28.0);
                    ui.set_max_width(520.0);
                    ui.heading("Recent files");
                    if self.recent_files.is_empty() {
                        ui.label("Your recently opened or saved documents will appear here.");
                    } else {
                        let recent = self.recent_files.clone();
                        for path in recent {
                            let label = path.file_name().and_then(|name| name.to_str()).unwrap_or("Untitled");
                            if ui.add_sized([ui.available_width(), 30.0], egui::Button::new(format!("▤  {label}   ·   {}", path.parent().unwrap_or(&path).display()))).clicked() {
                                self.load_path(path);
                            }
                        }
                    }
                });
            } else if self.show_preview {
                ui.columns(2, |columns| {
                    columns[0].vertical(|ui| {
                        ui.label("Markdown");
                        let output = egui::TextEdit::multiline(&mut self.text).font(egui::FontId::monospace(self.editor_font_size)).text_color(self.editor_color).desired_width(f32::INFINITY).desired_rows(20).code_editor().show(ui);
                        if let Some(range) = output.cursor_range { self.selection = (range.primary.index, range.secondary.index); }
                    });
                    columns[1].vertical(|ui| {
                        ui.label("Preview");
                        egui::ScrollArea::vertical().show(ui, |ui| for block in markdown_preview_blocks(&self.text) {
                            match block.kind {
                                PreviewKind::Heading(level) => { ui.label(egui::RichText::new(block.text).size(heading_size(level)).strong()); }
                                PreviewKind::Code => { ui.label(egui::RichText::new(block.text).monospace()); }
                                PreviewKind::Quote => { ui.label(egui::RichText::new(block.text).italics()); }
                                PreviewKind::ListItem | PreviewKind::Paragraph => { ui.label(block.text); }
                            }
                            ui.add_space(4.0);
                        });
                    });
                });
            } else {
                let output = egui::TextEdit::multiline(&mut self.text).font(egui::FontId::monospace(self.editor_font_size)).text_color(self.editor_color).desired_width(f32::INFINITY).desired_rows(20).code_editor().show(ui);
                if let Some(range) = output.cursor_range { self.selection = (range.primary.index, range.secondary.index); }
            }
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if !self.show_start { ui.label(format!("{} words", self.word_count())); ui.separator(); }
                ui.label(self.path.as_ref().and_then(|p| p.file_name()).and_then(|n| n.to_str()).unwrap_or(if self.show_start { "Start" } else { "Untitled" }));
                if self.is_dirty() { ui.separator(); ui.label("Unsaved changes"); }
                if let Some(notice) = &self.notice { ui.separator(); ui.label(notice); }
                if let Some(error) = &self.error { ui.separator(); ui.colored_label(egui::Color32::LIGHT_RED, error); }
            });
        });

        if let Some(action) = self.pending_action {
            egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO).show(ctx, |ui| {
                ui.label("You have unsaved changes. What would you like to do?");
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() { self.save_file(false); if !self.is_dirty() { self.pending_action = None; self.perform_action(action); } }
                    if ui.button("Discard").clicked() { self.pending_action = None; self.perform_action(action); }
                    if ui.button("Cancel").clicked() { self.pending_action = None; }
                });
            });
        }
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([850.0, 580.0]).with_min_inner_size([520.0, 360.0]),
        ..Default::default()
    };
    eframe::run_native("JotPad", options, Box::new(|_creation_context| Ok(Box::<JotPad>::default())))
}
