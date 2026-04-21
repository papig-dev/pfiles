use crate::core::archive;
use crate::core::archive::ArchiveEntry;
use crate::core::fs_ops;
use crate::core::fs_ops::FileEntry;
use crate::core::preview;
use crate::core::preview::PreviewData;
use chrono::Local;
use eframe::egui;
use egui_extras::{Column, TableBuilder};
use std::path::PathBuf;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Left,
    Right,
}

#[derive(Default)]
struct PaneState {
    path: String,
    path_input: String,
    entries: Vec<FileEntry>,
    selected: Option<usize>,
    error: Option<String>,
    scroll_to_selected: bool,
}

enum ModalState {
    Rename { target: String, input: String },
    Mkdir { input: String },
    ConfirmDelete { target: String, to_trash: bool },
}

pub struct PFilesApp {
    left: PaneState,
    right: PaneState,
    active: Side,
    show_hidden: bool,
    status: String,
    preview_path: Option<String>,
    preview_data: Option<PreviewData>,
    preview_texture: Option<egui::TextureHandle>,
    show_preview: bool,
    archive_entries: Vec<ArchiveEntry>,
    show_archive_panel: bool,
    modal: Option<ModalState>,
}

impl PFilesApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);
        let initial = fs_ops::home_dir().unwrap_or_else(|_| "/".to_string());

        let mut app = Self {
            left: PaneState {
                path: initial.clone(),
                path_input: initial.clone(),
                ..Default::default()
            },
            right: PaneState {
                path: initial.clone(),
                path_input: initial,
                ..Default::default()
            },
            active: Side::Left,
            show_hidden: false,
            status: "Ready".to_string(),
            preview_path: None,
            preview_data: None,
            preview_texture: None,
            show_preview: true,
            archive_entries: Vec::new(),
            show_archive_panel: false,
            modal: None,
        };
        app.reload(Side::Left);
        app.reload(Side::Right);
        app
    }

    fn pane(&self, side: Side) -> &PaneState {
        match side {
            Side::Left => &self.left,
            Side::Right => &self.right,
        }
    }

    fn pane_mut(&mut self, side: Side) -> &mut PaneState {
        match side {
            Side::Left => &mut self.left,
            Side::Right => &mut self.right,
        }
    }

    fn reload(&mut self, side: Side) {
        let show_hidden = self.show_hidden;
        let pane = self.pane_mut(side);
        match fs_ops::list_dir(&pane.path, show_hidden) {
            Ok(entries) => {
                pane.entries = entries;
                pane.path_input = pane.path.clone();
                pane.error = None;
                pane.selected = if pane.entries.is_empty() { None } else { Some(0) };
            }
            Err(err) => {
                pane.error = Some(err.clone());
                pane.entries.clear();
                pane.selected = None;
            }
        }
    }

    fn navigate_to(&mut self, side: Side, path: String) {
        let pane = self.pane_mut(side);
        pane.path = path;
        self.reload(side);
    }

    fn go_parent(&mut self, side: Side) {
        let current = self.pane(side).path.clone();
        if let Some(parent) = fs_ops::path_parent(&current) {
            self.navigate_to(side, parent);
        }
    }

    fn selected_entry(&self, side: Side) -> Option<&FileEntry> {
        let pane = self.pane(side);
        pane.selected.and_then(|idx| pane.entries.get(idx))
    }

    fn selected_path(&self, side: Side) -> Option<String> {
        self.selected_entry(side).map(|e| e.path.clone())
    }

    fn has_selection(&self, side: Side) -> bool {
        self.selected_entry(side).is_some()
    }

    fn can_go_parent(&self, side: Side) -> bool {
        fs_ops::path_parent(&self.pane(side).path).is_some()
    }

    fn other_side(side: Side) -> Side {
        match side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }

    fn move_selection(&mut self, side: Side, delta: isize) {
        let pane = self.pane_mut(side);
        let len = pane.entries.len();
        if len == 0 {
            pane.selected = None;
            return;
        }
        let current = pane.selected.unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, (len as isize) - 1) as usize;
        pane.selected = Some(next);
        pane.scroll_to_selected = true;
    }

    fn preview_selected(&mut self, ctx: &egui::Context, side: Side) {
        let Some(entry) = self.selected_entry(side).cloned() else {
            self.status = "No selected item".to_string();
            return;
        };
        if entry.is_dir {
            self.status = "Preview works for files only".to_string();
            self.preview_path = None;
            self.preview_data = None;
            self.preview_texture = None;
            return;
        }
        self.load_preview(ctx, entry.path);
    }

    fn copy_or_move_selected(&mut self, move_mode: bool) {
        let side = self.active;
        let other = Self::other_side(side);
        let Some(source) = self.selected_path(side) else {
            self.status = "No selected item".to_string();
            return;
        };
        let dest_dir = self.pane(other).path.clone();
        let sources = vec![source.clone()];

        let result = if move_mode {
            fs_ops::move_paths(&sources, &dest_dir)
        } else {
            fs_ops::copy_paths(&sources, &dest_dir)
        };

        match result {
            Ok(()) => {
                self.reload(side);
                self.reload(other);
                self.status = if move_mode {
                    format!("Moved: {}", source)
                } else {
                    format!("Copied: {}", source)
                };
            }
            Err(err) => {
                self.status = format!("Action failed: {}", err);
            }
        }
    }

    fn open_rename_modal(&mut self) {
        let Some(target) = self.selected_path(self.active) else {
            self.status = "No selected item".to_string();
            return;
        };
        let name = PathBuf::from(&target)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        self.modal = Some(ModalState::Rename {
            target,
            input: name,
        });
    }

    fn open_mkdir_modal(&mut self) {
        self.modal = Some(ModalState::Mkdir {
            input: String::new(),
        });
    }

    fn open_delete_modal(&mut self) {
        let Some(target) = self.selected_path(self.active) else {
            self.status = "No selected item".to_string();
            return;
        };
        self.modal = Some(ModalState::ConfirmDelete {
            target,
            to_trash: true,
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.modal.is_some() {
            return;
        }

        // Remove Tab from both the event queue and keys_down so egui's traversal sees nothing
        let tab_pressed = ctx.input_mut(|i| {
            let hit = i.key_pressed(egui::Key::Tab) && i.modifiers == egui::Modifiers::NONE;
            if hit {
                i.events.retain(|e| {
                    !matches!(
                        e,
                        egui::Event::Key {
                            key: egui::Key::Tab,
                            pressed: true,
                            ..
                        }
                    )
                });
                i.keys_down.remove(&egui::Key::Tab);
            }
            hit
        });
        if tab_pressed {
            self.active = Self::other_side(self.active);
            // Point focus at a phantom ID that is never rendered → no widget claims focus next frame
            ctx.memory_mut(|m| m.request_focus(egui::Id::new("__pfiles_panel_focus__")));
        }

        let wants_text_input = ctx.wants_keyboard_input();

        if !wants_text_input {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                self.move_selection(self.active, 1);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                self.move_selection(self.active, -1);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
                self.open_selected(ctx, self.active);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Backspace)) {
                self.go_parent(self.active);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Space)) {
                self.preview_selected(ctx, self.active);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::H)) {
                self.show_hidden = !self.show_hidden;
                self.reload(Side::Left);
                self.reload(Side::Right);
            }
        }

        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F2)) {
            self.open_rename_modal();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F5)) {
            self.copy_or_move_selected(false);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F6)) {
            self.copy_or_move_selected(true);
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F7)) {
            self.open_mkdir_modal();
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::F8)) {
            self.open_delete_modal();
        }
    }

    fn render_modal(&mut self, ctx: &egui::Context) {
        let Some(modal) = self.modal.take() else {
            return;
        };

        match modal {
            ModalState::Rename { target, mut input } => {
                let mut submit = false;
                let mut keep_open = true;
                egui::Window::new("Rename (F2)")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.monospace(&target);
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut input)
                                .hint_text("new name")
                                .desired_width(320.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submit = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                keep_open = false;
                            }
                            if ui.button("Rename").clicked() {
                                submit = true;
                            }
                        });
                    });

                if submit {
                    if input.trim().is_empty() {
                        self.status = "Name cannot be empty".to_string();
                        self.modal = Some(ModalState::Rename { target, input });
                        return;
                    }
                    let from = PathBuf::from(&target);
                    let Some(parent) = from.parent() else {
                        self.status = "Rename failed: invalid target".to_string();
                        return;
                    };
                    let to = parent.join(input.trim());
                    match fs_ops::rename_path(&target, &to.to_string_lossy()) {
                        Ok(()) => {
                            self.reload(self.active);
                            self.status = format!("Renamed to {}", to.to_string_lossy());
                        }
                        Err(err) => {
                            self.status = format!("Rename failed: {}", err);
                        }
                    }
                } else if keep_open {
                    self.modal = Some(ModalState::Rename { target, input });
                }
            }
            ModalState::Mkdir { mut input } => {
                let mut submit = false;
                let mut keep_open = true;
                egui::Window::new("Create Folder (F7)")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("Folder name");
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut input)
                                .hint_text("new folder")
                                .desired_width(320.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            submit = true;
                        }
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                keep_open = false;
                            }
                            if ui.button("Create").clicked() {
                                submit = true;
                            }
                        });
                    });

                if submit {
                    if input.trim().is_empty() {
                        self.status = "Folder name cannot be empty".to_string();
                        self.modal = Some(ModalState::Mkdir { input });
                        return;
                    }
                    let base = self.pane(self.active).path.clone();
                    let path = fs_ops::path_join(&base, input.trim());
                    match fs_ops::make_dir(&path) {
                        Ok(()) => {
                            self.reload(self.active);
                            self.status = format!("Created folder: {}", path);
                        }
                        Err(err) => {
                            self.status = format!("Create failed: {}", err);
                        }
                    }
                } else if keep_open {
                    self.modal = Some(ModalState::Mkdir { input });
                }
            }
            ModalState::ConfirmDelete {
                target,
                mut to_trash,
            } => {
                let mut submit = false;
                let mut keep_open = true;
                egui::Window::new("Delete (F8)")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label("Delete selected item?");
                        ui.monospace(&target);
                        ui.checkbox(&mut to_trash, "Move to trash");
                        ui.horizontal(|ui| {
                            if ui.button("Cancel").clicked() {
                                keep_open = false;
                            }
                            if ui.button("Delete").clicked() {
                                submit = true;
                            }
                        });
                    });

                if submit {
                    let paths = vec![target.clone()];
                    match fs_ops::delete_paths(&paths, to_trash) {
                        Ok(()) => {
                            self.reload(self.active);
                            self.status = format!("Deleted: {}", target);
                        }
                        Err(err) => {
                            self.status = format!("Delete failed: {}", err);
                        }
                    }
                } else if keep_open {
                    self.modal = Some(ModalState::ConfirmDelete { target, to_trash });
                }
            }
        }
    }

    fn open_selected(&mut self, ctx: &egui::Context, side: Side) {
        let Some(entry) = self.selected_entry(side).cloned() else {
            return;
        };

        if entry.is_dir {
            self.navigate_to(side, entry.path.clone());
            self.status = format!("Opened directory: {}", entry.path);
            return;
        }

        if archive::is_archive(&entry.path) {
            match archive::list_archive(&entry.path) {
                Ok(list) => {
                    self.archive_entries = list;
                    self.show_archive_panel = true;
                    self.status = format!("Archive loaded: {}", entry.path);
                }
                Err(err) => {
                    self.status = format!("Archive error: {}", err);
                }
            }
            return;
        }

        self.load_preview(ctx, entry.path.clone());
    }

    fn load_preview(&mut self, ctx: &egui::Context, path: String) {
        self.preview_texture = None;
        match preview::preview_file(&path) {
            Ok(data) => {
                if let PreviewData::Image { bytes, .. } = &data {
                    self.preview_texture = load_texture_from_bytes(ctx, &path, bytes);
                }
                self.preview_path = Some(path.clone());
                self.preview_data = Some(data);
                self.status = format!("Preview loaded: {}", path);
            }
            Err(err) => {
                self.preview_path = None;
                self.preview_data = None;
                self.status = format!("Preview error: {}", err);
            }
        }
    }

    fn render_toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            if ui.button("Reload").clicked() {
                self.reload(Side::Left);
                self.reload(Side::Right);
                self.status = "Reloaded".to_string();
            }
            if ui
                .add_enabled(self.can_go_parent(self.active), egui::Button::new("Up"))
                .clicked()
            {
                self.go_parent(self.active);
                self.status = "Moved to parent directory".to_string();
            }
            if ui
                .add_enabled(self.has_selection(self.active), egui::Button::new("Open"))
                .clicked()
            {
                self.open_selected(ctx, self.active);
            }
            let preview_panel_btn = if self.show_preview {
                "Hide Preview"
            } else {
                "Preview"
            };
            if ui.button(preview_panel_btn).clicked() {
                self.show_preview = !self.show_preview;
                if self.show_preview {
                    self.preview_selected(ctx, self.active);
                } else {
                    self.status = "Preview panel hidden".to_string();
                }
            }
            ui.separator();
            let hidden_label = if self.show_hidden {
                "Hide dotfiles"
            } else {
                "Show dotfiles"
            };
            if ui.button(hidden_label).clicked() {
                self.show_hidden = !self.show_hidden;
                self.reload(Side::Left);
                self.reload(Side::Right);
                self.status = if self.show_hidden {
                    "Dotfiles are now visible".to_string()
                } else {
                    "Dotfiles are now hidden".to_string()
                };
            }
        });
    }

    fn render_pane(&mut self, ui: &mut egui::Ui, side: Side, ctx: &egui::Context) {
        let title = if side == Side::Left { "Left" } else { "Right" };
        let is_active = self.active == side;
        let (mut path_input, entries, mut selected, error, scroll_to_selected) = {
            let pane = self.pane(side);
            (
                pane.path_input.clone(),
                pane.entries.clone(),
                pane.selected,
                pane.error.clone(),
                pane.scroll_to_selected,
            )
        };
        let mut trigger_go = false;
        let mut clicked_idx: Option<usize> = None;
        let mut double_clicked_idx: Option<usize> = None;

        let mut frame = egui::Frame::group(ui.style());
        if is_active {
            frame = frame.stroke(egui::Stroke::new(1.5, ui.visuals().selection.bg_fill));
        }

        frame.show(ui, |ui| {
            let (active_rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), 3.0),
                egui::Sense::hover(),
            );
            let bar_color = if is_active {
                ui.visuals().selection.bg_fill
            } else {
                ui.visuals().widgets.noninteractive.bg_stroke.color
            };
            ui.painter().rect_filled(active_rect, 0.0, bar_color);

            ui.horizontal(|ui| {
                let title_text = if is_active {
                    format!("● {}", title)
                } else {
                    title.to_string()
                };
                let mut rt = egui::RichText::new(title_text).strong();
                if is_active {
                    rt = rt.color(ui.visuals().selection.bg_fill);
                }
                ui.label(rt);
                if is_active {
                    ui.label("(Active)");
                }
            });

            ui.horizontal(|ui| {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut path_input)
                        .hint_text("path")
                        .desired_width(f32::INFINITY),
                );
                let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("Go").clicked() || enter {
                    trigger_go = true;
                }
            });

            if let Some(err) = &error {
                ui.colored_label(ui.visuals().error_fg_color, err);
            }

            // Snapshot colors before TableBuilder borrows ui
            let sel_fg = ui.visuals().selection.stroke.color;
            let normal_fg = ui.visuals().text_color();

            let mut builder = TableBuilder::new(ui)
                .id_salt(format!("pane_table_{:?}", side))
                .striped(true)
                .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                .column(Column::exact(36.0))
                .column(Column::remainder().resizable(false).clip(true))
                .column(Column::initial(80.0).resizable(true).clip(true))
                .column(Column::initial(130.0).resizable(true).clip(true));

            if scroll_to_selected {
                if let Some(row_idx) = selected {
                    builder = builder.scroll_to_row(row_idx, None);
                }
            }

            builder
                .header(20.0, |mut header| {
                    header.col(|ui| { ui.strong("Type"); });
                    header.col(|ui| { ui.strong("Name"); });
                    header.col(|ui| { ui.strong("Size"); });
                    header.col(|ui| { ui.strong("Modified"); });
                })
                .body(|body| {
                    body.rows(22.0, entries.len(), |mut row| {
                        let idx = row.index();
                        let entry = &entries[idx];
                        let is_selected = selected == Some(idx);

                        let ty = if entry.is_dir { "DIR" } else { "FILE" };
                        let mtime = entry.modified.map(fmt_mtime).unwrap_or_else(|| "-".to_string());
                        let size = if entry.is_dir { "-".to_string() } else { format_size(entry.size) };
                        let name: String = if entry.is_symlink {
                            format!("↗ {}", entry.name)
                        } else {
                            entry.name.clone()
                        }.nfc().collect();

                        let text_color = if is_selected { sel_fg } else { normal_fg };

                        row.set_selected(is_selected);

                        // row.col() returns (Rect, Response) where the Response uses Sense::hover()
                        // so we must capture the inner Label's response for click detection
                        let clicked = std::cell::Cell::new(false);
                        let double_clicked = std::cell::Cell::new(false);

                        let sense = egui::Sense::click();
                        row.col(|ui| {
                            let r = ui.add(egui::Label::new(egui::RichText::new(ty).color(text_color)).truncate().sense(sense));
                            if r.clicked() { clicked.set(true); }
                            if r.double_clicked() { double_clicked.set(true); }
                        });
                        row.col(|ui| {
                            let r = ui.add(egui::Label::new(egui::RichText::new(name.as_str()).color(text_color)).truncate().sense(sense));
                            if r.clicked() { clicked.set(true); }
                            if r.double_clicked() { double_clicked.set(true); }
                        });
                        row.col(|ui| {
                            let r = ui.add(egui::Label::new(egui::RichText::new(size.as_str()).color(text_color)).truncate().sense(sense));
                            if r.clicked() { clicked.set(true); }
                            if r.double_clicked() { double_clicked.set(true); }
                        });
                        row.col(|ui| {
                            let r = ui.add(egui::Label::new(egui::RichText::new(mtime.as_str()).color(text_color)).truncate().sense(sense));
                            if r.clicked() { clicked.set(true); }
                            if r.double_clicked() { double_clicked.set(true); }
                        });

                        if clicked.get() { clicked_idx = Some(idx); }
                        if double_clicked.get() { double_clicked_idx = Some(idx); }
                    });
                });
        });

        if let Some(idx) = clicked_idx {
            selected = Some(idx);
            self.active = side;
        }
        if let Some(idx) = double_clicked_idx {
            selected = Some(idx);
            self.active = side;
        }

        {
            let pane = self.pane_mut(side);
            pane.path_input = path_input.clone();
            pane.selected = selected;
            pane.scroll_to_selected = false;
        }

        if trigger_go {
            self.navigate_to(side, path_input);
        }

        if double_clicked_idx.is_some() {
            self.open_selected(ctx, side);
        }
    }

    fn render_preview_panel(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview");
        if let Some(path) = &self.preview_path {
            ui.monospace(path);
        }
        ui.separator();

        match &self.preview_data {
            Some(PreviewData::Text { content, truncated }) => {
                if *truncated {
                    ui.label("Text preview is truncated (512KB)");
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.code(content);
                });
            }
            Some(PreviewData::Image { bytes, .. }) => {
                if let Some(texture) = &self.preview_texture {
                    let available = ui.available_size();
                    let img_size = texture.size_vec2();
                    let scale = (available.x / img_size.x)
                        .min(available.y / img_size.y)
                        .min(1.0)
                        .max(0.1);
                    ui.image((texture.id(), img_size * scale));
                } else {
                    ui.label(format!("Image decode failed ({} bytes)", bytes.len()));
                }
            }
            Some(PreviewData::Binary { size, mime }) => {
                ui.label(format!("Binary file"));
                ui.label(format!("MIME: {}", mime));
                ui.label(format!("Size: {}", format_size(*size)));
            }
            None => {
                ui.label("No preview loaded");
            }
        }

        if self.show_archive_panel {
            ui.separator();
            ui.heading(format!("Archive entries ({})", self.archive_entries.len()));
            egui::ScrollArea::vertical()
                .max_height(240.0)
                .show(ui, |ui| {
                    for ent in &self.archive_entries {
                        let ty = if ent.is_dir { "DIR" } else { "FILE" };
                        ui.label(format!("{:<4} {:>10}  {}", ty, format_size(ent.size), ent.name));
                    }
                });
        }
    }
}

impl eframe::App for PFilesApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            self.render_toolbar(ui, ctx);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Active: {}", if self.active == Side::Left { "left" } else { "right" }));
                ui.separator();
                ui.label(&self.status);
            });
        });

        if self.show_preview {
            egui::SidePanel::right("preview")
                .resizable(true)
                .default_width(420.0)
                .show(ctx, |ui| {
                    self.render_preview_panel(ui);
                });
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |cols| {
                let left_rect = cols[0].max_rect();
                let right_rect = cols[1].max_rect();
                cols[0].set_clip_rect(left_rect);
                cols[1].set_clip_rect(right_rect);
                self.render_pane(&mut cols[0], Side::Left, ctx);
                self.render_pane(&mut cols[1], Side::Right, ctx);
            });
        });

        self.render_modal(ctx);
    }
}

fn load_texture_from_bytes(
    ctx: &egui::Context,
    path: &str,
    bytes: &[u8],
) -> Option<egui::TextureHandle> {
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();
    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
    Some(ctx.load_texture(path.to_string(), color_image, egui::TextureOptions::LINEAR))
}

fn format_size(v: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let f = v as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.1} MB", f / MB)
    } else if f >= KB {
        format!("{:.1} KB", f / KB)
    } else {
        format!("{} B", v)
    }
}

fn fmt_mtime(ts: i64) -> String {
    let dt = chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.with_timezone(&Local))
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string());
    dt.unwrap_or_else(|| "-".to_string())
}

fn configure_fonts(ctx: &egui::Context) {
    let fonts_to_register = load_font_chain();
    if fonts_to_register.is_empty() {
        return;
    }

    let mut fonts = egui::FontDefinitions::default();
    let mut names = Vec::with_capacity(fonts_to_register.len());
    for (name, bytes) in fonts_to_register {
        fonts
            .font_data
            .insert(name.clone(), egui::FontData::from_owned(bytes).into());
        names.push(name);
    }

    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        for name in names.iter().rev() {
            family.insert(0, name.clone());
        }
    }
    if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        for name in &names {
            family.push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}

fn load_font_chain() -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();

    let bundled = [
        (
            "jbmh_nerd",
            vec![
                "assets/fonts/JetBrainsMonoHangulNerdFont-Regular.ttf",
                "assets/fonts/JetBrainsMonoHangulNerdFontMono-Regular.ttf",
                "assets/fonts/JetBrainsMonoHangulNerdFont.ttc",
                "assets/fonts/JetBrainsMonoHangulNerdFontMono.ttc",
            ],
        ),
        (
            "jbmh",
            vec![
                "assets/fonts/JetBrainsMonoHangul-Regular.ttf",
                "assets/fonts/JetBrainsMonoHangul-Medium.ttf",
                "assets/fonts/JetBrainsMonoHangul.ttc",
            ],
        ),
    ];

    for (name, rel_paths) in bundled {
        if let Some(bytes) = load_from_relative_candidates(&rel_paths) {
            out.push((name.to_string(), bytes));
        }
    }

    if let Some((name, bytes)) = load_system_kr_fallback_font() {
        out.push((name, bytes));
    }

    out
}

fn load_from_relative_candidates(rel_paths: &[&str]) -> Option<Vec<u8>> {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()));

    for rel in rel_paths {
        let mut abs_candidates: Vec<PathBuf> = Vec::new();
        abs_candidates.push(PathBuf::from(rel));

        if let Some(dir) = &exe_dir {
            abs_candidates.push(dir.join(rel));
            abs_candidates.push(dir.join("..").join(rel));
            abs_candidates.push(dir.join("..").join("..").join(rel));
            abs_candidates.push(dir.join("..").join("..").join("..").join(rel));

            #[cfg(target_os = "macos")]
            {
                abs_candidates.push(
                    dir.join("..").join("Resources").join(rel.replace("assets/", "")),
                );
            }
        }

        for path in abs_candidates {
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(bytes);
            }
        }
    }
    None
}

fn load_system_kr_fallback_font() -> Option<(String, Vec<u8>)> {
    #[cfg(target_os = "macos")]
    let candidates = [
        ("nanum_gothic", "/Library/Fonts/NanumGothic.ttf"),
        ("nanum_myeongjo", "/Library/Fonts/NanumMyeongjo.ttf"),
        ("apple_myungjo", "/System/Library/Fonts/Supplemental/AppleMyungjo.ttf"),
    ];

    #[cfg(target_os = "windows")]
    let candidates = [
        ("malgun", "C:\\Windows\\Fonts\\malgun.ttf"),
        ("gulim", "C:\\Windows\\Fonts\\gulim.ttc"),
        ("batang", "C:\\Windows\\Fonts\\batang.ttc"),
    ];

    #[cfg(target_os = "linux")]
    let candidates = [
        ("nanum_gothic", "/usr/share/fonts/truetype/nanum/NanumGothic.ttf"),
        ("nanum_myeongjo", "/usr/share/fonts/truetype/nanum/NanumMyeongjo.ttf"),
        ("noto_cjk", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
    ];

    for (name, path) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Some((name.to_string(), bytes));
        }
    }
    None
}
