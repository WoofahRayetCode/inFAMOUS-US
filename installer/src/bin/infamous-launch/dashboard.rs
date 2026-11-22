use super::*;
use eframe::egui::{self, Color32, RichText};
use infamous_installer::{game, lock, setup, trophy, update};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};

const BACKGROUND: Color32 = Color32::from_rgb(16, 19, 25);
const PANEL: Color32 = Color32::from_rgb(24, 30, 39);
const BORDER: Color32 = Color32::from_rgb(47, 58, 73);
const MUTED: Color32 = Color32::from_rgb(159, 174, 193);
const BLUE: Color32 = Color32::from_rgb(105, 197, 255);
const AMBER: Color32 = Color32::from_rgb(255, 199, 107);

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Play,
    Settings,
    Saves,
    Achievements,
    Support,
}

struct Dashboard {
    state: State,
    tab: Tab,
    played: Arc<AtomicBool>,
    feature_check: Option<mpsc::Receiver<Features>>,
    banner: Option<egui::TextureHandle>,
    region: Option<String>,
    notice: Option<(String, bool)>,
    saves: Result<Vec<save_manager::Entry>, String>,
    backups: Result<Vec<save_manager::Entry>, String>,
    backup: Option<String>,
    scope: Scope,
    restore: Option<(String, Scope)>,
    save_transaction: Option<SaveTransaction>,
    trophy_catalog: Option<Result<trophy::Catalog, String>>,
    trophy_progress: Result<BTreeMap<u32, u64>, String>,
    update: UpdateState,
}

use infamous_installer::save_manager;

enum UpdateState {
    Disabled,
    Idle,
    Checking {
        completion: mpsc::Receiver<Result<update::Preview, String>>,
        manual: bool,
        open_review: bool,
    },
    Checked {
        result: Result<update::Preview, String>,
        manual: bool,
    },
    Reviewing(update::Preview),
    Installing {
        installer: setup::Installer,
        launcher: PathBuf,
    },
    Failed {
        error: String,
        log: String,
    },
    RestartFailed {
        launcher: PathBuf,
        error: String,
    },
}

impl UpdateState {
    fn blocks_dashboard(&self) -> bool {
        matches!(
            self,
            Self::Reviewing(_)
                | Self::Installing { .. }
                | Self::Failed { .. }
                | Self::RestartFailed { .. }
        )
    }

    fn defer_review(&mut self) {
        if let Self::Checking { open_review, .. } = self {
            *open_review = false;
        }
    }
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

enum Snapshot {
    Backup(Scope),
    Restore(String, Scope),
}

struct SaveLists {
    saves: Result<Vec<save_manager::Entry>, String>,
    backups: Result<Vec<save_manager::Entry>, String>,
}

impl SaveLists {
    fn read(manager: &save_manager::Manager) -> Self {
        Self {
            saves: manager.list_saves().map_err(|e| e.to_string()),
            backups: manager.list_backups().map_err(|e| e.to_string()),
        }
    }
}

struct SnapshotCompletion {
    result: Result<SnapshotOutcome, String>,
    lists: SaveLists,
}

enum SnapshotOutcome {
    BackedUp,
    Restored(BTreeMap<String, String>),
}

struct SaveTransaction {
    progress: &'static str,
    completion: mpsc::Receiver<SnapshotCompletion>,
}

fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(PANEL)
        .stroke(egui::Stroke::new(1.0_f32, BORDER))
        .rounding(8.0)
        .inner_margin(18.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui);
        });
}

fn muted(ui: &mut egui::Ui, text: impl Into<String>) {
    ui.label(RichText::new(text.into()).color(MUTED));
}

fn scope_name(scope: Scope) -> &'static str {
    match scope {
        Scope::Saves => "Saves only",
        Scope::Settings => "Settings only",
        Scope::Both => "Saves and settings",
    }
}

impl Dashboard {
    fn new(state: State, played: Arc<AtomicBool>, ctx: &egui::Context) -> Self {
        ctx.set_theme(egui::Theme::Dark);
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BACKGROUND;
        visuals.window_fill = PANEL;
        visuals.window_stroke = egui::Stroke::new(1.0_f32, BORDER);
        visuals.override_text_color = Some(Color32::from_rgb(231, 238, 248));
        visuals.selection.bg_fill = Color32::from_rgb(29, 80, 116);
        visuals.hyperlink_color = BLUE;
        visuals.widgets.inactive.bg_fill = Color32::from_rgb(32, 41, 53);
        visuals.widgets.inactive.weak_bg_fill = visuals.widgets.inactive.bg_fill;
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(42, 57, 75);
        visuals.widgets.hovered.weak_bg_fill = visuals.widgets.hovered.bg_fill;
        ctx.set_visuals(visuals);
        ctx.style_mut(|style| {
            style.spacing.item_spacing = egui::vec2(10.0, 9.0);
            style.spacing.button_padding = egui::vec2(12.0, 7.0);
            style.spacing.interact_size.y = 28.0;
            for text_style in [egui::TextStyle::Body, egui::TextStyle::Button] {
                if let Some(font) = style.text_styles.get_mut(&text_style) {
                    font.size = 15.0;
                }
            }
        });
        let feature_check = if state.features.is_none() {
            let (send, receive) = mpsc::channel();
            let exe = state.launch.exe.clone();
            let cache = state.launch.cache_path();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let _ = send.send(scan_features(&exe, &cache));
                ctx.request_repaint();
            });
            Some(receive)
        } else {
            None
        };
        let region = state
            .launch
            .args
            .first()
            .and_then(|p| p.parent()?.parent())
            .and_then(|p| game::read_sfo(&p.join("PARAM.SFO")).ok())
            .and_then(|mut sfo| sfo.remove("TITLE_ID"));
        let tab = if state.launch.settings_only {
            Tab::Settings
        } else {
            Tab::Play
        };
        let installed = !state.launch.settings_only && setup::is_installed(&state.launch.root);
        let mut dashboard = Self {
            state,
            tab,
            played,
            feature_check,
            banner: banner_texture(ctx),
            region,
            notice: None,
            saves: Ok(Vec::new()),
            backups: Ok(Vec::new()),
            backup: None,
            scope: Scope::Saves,
            restore: None,
            save_transaction: None,
            trophy_catalog: None,
            trophy_progress: Ok(BTreeMap::new()),
            update: if installed {
                UpdateState::Idle
            } else {
                UpdateState::Disabled
            },
        };
        if installed {
            dashboard.start_update_check(ctx, false);
        }
        dashboard
    }

    fn region_label(&self) -> &str {
        match self.region.as_deref() {
            Some("BCUS98119") => "US / BCUS98119",
            Some("BCES00220") => "Europe / BCES00220",
            Some(other) => other,
            None => "Region unavailable",
        }
    }

    fn readiness(&self) -> Result<&'static str, &'static str> {
        if self.state.launch.settings_only {
            return Ok("Settings editor");
        }
        if !self.state.launch.exe.is_file() {
            return Err("Game build missing. Run inFamous Setup to repair it.");
        }
        if self.state.launch.args.iter().any(|p| !p.is_file()) {
            return Err("Game input missing. Reconnect the disc folder or run Setup.");
        }
        Ok("Game build ready")
    }

    fn summary(&self) -> String {
        format!(
            "{}   /   {}   /   {}",
            DISPLAY[2].names[self.state.get(&DISPLAY[2])],
            DISPLAY[4].names[self.state.get(&DISPLAY[4])],
            DISPLAY[0].names[self.state.get(&DISPLAY[0])]
        )
    }

    fn setting(&mut self, ui: &mut egui::Ui, row: &Row) {
        let mut selected = self.state.get(row);
        ui.horizontal(|ui| {
            ui.add_sized(
                [145.0, 0.0],
                egui::Label::new(RichText::new(row.label).strong()),
            )
            .on_hover_text(row.hint);
            egui::ComboBox::from_id_salt(row.key)
                .selected_text(row.names[selected])
                .width(175.0)
                .show_ui(ui, |ui| {
                    for (i, name) in row.names.iter().enumerate() {
                        ui.selectable_value(&mut selected, i, *name);
                    }
                })
                .response
                .on_hover_text(row.hint);
        });
        if selected != self.state.get(row) {
            self.state.cur.insert(row.key, selected);
        }
    }

    fn home(&mut self, ui: &mut egui::Ui) {
        let banner_height = (ui.ctx().screen_rect().height() * 0.37).clamp(200.0, 285.0);
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), banner_height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8.0, Color32::from_rgb(19, 42, 64));
        if let Some(banner) = &self.banner {
            let ratio = rect.width() / rect.height();
            let texture_ratio = banner.size_vec2().x / banner.size_vec2().y;
            let width = (ratio / texture_ratio).min(1.0);
            let height = (texture_ratio / ratio).min(1.0);
            let mid = egui::pos2(0.5, 0.5);
            painter.image(
                banner.id(),
                rect,
                egui::Rect::from_center_size(mid, egui::vec2(width, height)),
                Color32::WHITE,
            );
        }
        let mut shade = egui::Mesh::default();
        shade.colored_vertex(rect.left_top(), Color32::from_black_alpha(216));
        shade.colored_vertex(rect.right_top(), Color32::from_black_alpha(22));
        shade.colored_vertex(rect.right_bottom(), Color32::from_black_alpha(87));
        shade.colored_vertex(rect.left_bottom(), Color32::from_black_alpha(226));
        shade.add_triangle(0, 1, 2);
        shade.add_triangle(0, 2, 3);
        painter.add(egui::Shape::mesh(shade));
        painter.rect_filled(
            egui::Rect::from_min_size(
                rect.left_top() + egui::vec2(25.0, 47.0),
                egui::vec2(28.0, 3.0),
            ),
            1.0,
            BLUE,
        );
        painter.text(
            rect.left_top() + egui::vec2(25.0, 57.0),
            egui::Align2::LEFT_TOP,
            "inFamous",
            egui::FontId::proportional(50.0),
            Color32::WHITE,
        );
        painter.text(
            rect.left_top() + egui::vec2(28.0, 130.0),
            egui::Align2::LEFT_TOP,
            self.region_label(),
            egui::FontId::monospace(14.0),
            BLUE,
        );

        ui.add_space(8.0);
        card(ui, |ui| {
            ui.label(RichText::new("Installation").size(19.0).strong());
            let (status, color) = match self.readiness() {
                Ok(s) => (s, BLUE),
                Err(s) => (s, AMBER),
            };
            ui.label(RichText::new(status).color(color));
        });
        card(ui, |ui| {
            ui.label(RichText::new("Quick settings").size(19.0).strong());
            ui.add_enabled_ui(self.save_transaction.is_none(), |ui| {
                ui.columns(2, |cols| {
                    self.setting(&mut cols[0], &DISPLAY[0]);
                    self.setting(&mut cols[1], &DISPLAY[4]);
                });
            });
        });
    }

    fn settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Display & Controls");
        if self.feature_check.is_some() {
            muted(
                ui,
                "Checking which graphics features this build supports...",
            );
        }
        for (title, rows) in ["Display", "Graphics", "Controls", "Game"]
            .into_iter()
            .zip(self.state.groups())
        {
            card(ui, |ui| {
                ui.label(RichText::new(title).size(19.0).strong());
                for row in rows {
                    self.setting(ui, row);
                }
            });
        }
        if ui.button("Reset to defaults").clicked() {
            self.state.restore_defaults();
        }
    }

    fn refresh_saves(&mut self) {
        self.apply_save_lists(SaveLists::read(&self.state.launch.save_manager()));
    }

    fn apply_save_lists(&mut self, lists: SaveLists) {
        self.saves = lists.saves;
        self.backups = lists.backups;
        if let Ok(entries) = &self.backups {
            if self
                .backup
                .as_ref()
                .is_none_or(|name| !entries.iter().any(|e| &e.name == name))
            {
                self.backup = entries.first().map(|e| e.name.clone());
            }
        } else {
            self.backup = None;
        }
    }

    fn notify(&mut self, result: Result<String, String>) {
        self.notice = Some(match result {
            Ok(message) => (message, false),
            Err(message) => (message, true),
        });
    }

    fn open_data_folder(&mut self, path: &Path) {
        match std::fs::create_dir_all(path) {
            Ok(()) => setup::open_folder(path),
            Err(error) => self.notify(Err(format!("Could not open folder: {error}"))),
        }
    }

    fn start_save_transaction(&mut self, ctx: &egui::Context, snapshot: Snapshot) {
        if self.save_transaction.is_some() || self.update.blocks_dashboard() {
            return;
        }
        self.update.defer_review();
        let progress = match snapshot {
            Snapshot::Backup(_) => "Creating backup...",
            Snapshot::Restore(..) => "Restoring backup...",
        };
        let launch = self.state.launch.clone();
        let manager = launch.save_manager();
        let repaint = ctx.clone();
        let (send, completion) = mpsc::channel();
        let worker = std::thread::Builder::new().spawn(move || {
            let result = (|| {
                let _lock = GameLock::acquire().map_err(|e| e.to_string())?;
                match snapshot {
                    Snapshot::Backup(scope) => {
                        manager.backup(scope).map(|_| SnapshotOutcome::BackedUp)
                    }
                    Snapshot::Restore(name, scope) => manager
                        .restore(&name, scope)
                        .map(|_| SnapshotOutcome::Restored(State::read_ini(&launch))),
                }
                .map_err(|e| e.to_string())
            })();
            let lists = SaveLists::read(&manager);
            let _ = send.send(SnapshotCompletion { result, lists });
            repaint.request_repaint();
        });
        match worker {
            Ok(_) => {
                self.notice = None;
                self.save_transaction = Some(SaveTransaction {
                    progress,
                    completion,
                });
                ctx.request_repaint();
            }
            Err(error) => self.notify(Err(format!("Could not start backup or restore: {error}"))),
        }
    }

    fn poll_save_transaction(&mut self) {
        let Some(transaction) = &self.save_transaction else {
            return;
        };
        match transaction.completion.try_recv() {
            Ok(completion) => {
                self.save_transaction = None;
                let notice = match completion.result {
                    Ok(SnapshotOutcome::BackedUp) => Ok("Backup created.".into()),
                    Ok(SnapshotOutcome::Restored(ini)) => {
                        self.state.ini = ini;
                        self.state.effective.clear();
                        self.state.cur.clear();
                        self.state.sync_rows();
                        Ok("Restore complete. Recovery backup created.".into())
                    }
                    Err(error) => Err(error),
                };
                self.apply_save_lists(completion.lists);
                self.notify(notice);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.save_transaction = None;
                self.notify(Err("Backup or restore stopped unexpectedly.".into()));
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }

    fn saves(&mut self, ui: &mut egui::Ui) {
        let manager = self.state.launch.save_manager();
        ui.heading("Saves");
        card(ui, |ui| {
            ui.label(RichText::new("Game saves").size(19.0).strong());
            muted(ui, manager.saves.display().to_string());
            match &self.saves {
                Ok(entries) if entries.is_empty() => muted(ui, "No inFamous saves found."),
                Ok(entries) => {
                    egui::Grid::new("save_rows").striped(true).show(ui, |ui| {
                        ui.strong("Save");
                        ui.strong("Modified (UTC)");
                        ui.end_row();
                        for entry in entries {
                            ui.label(&entry.name);
                            ui.label(&entry.modified);
                            ui.end_row();
                        }
                    });
                }
                Err(error) => {
                    ui.colored_label(AMBER, format!("Could not read saves: {error}"));
                }
            }
            ui.horizontal_wrapped(|ui| {
                let idle = self.save_transaction.is_none();
                if ui
                    .add_enabled(idle, egui::Button::new("Open save folder"))
                    .clicked()
                {
                    self.open_data_folder(&manager.saves);
                }
                if ui.add_enabled(idle, egui::Button::new("Refresh")).clicked() {
                    self.refresh_saves();
                }
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Backups").size(19.0).strong());
            let dirty = self.state.dirty();
            ui.horizontal_wrapped(|ui| {
                for (label, scope) in [
                    ("Back up saves", Scope::Saves),
                    ("Back up settings", Scope::Settings),
                    ("Back up everything", Scope::Both),
                ] {
                    if ui
                        .add_enabled(
                            self.save_transaction.is_none()
                                && (!dirty || matches!(scope, Scope::Saves)),
                            egui::Button::new(label),
                        )
                        .clicked()
                    {
                        self.start_save_transaction(ui.ctx(), Snapshot::Backup(scope));
                    }
                }
                if ui
                    .add_enabled(
                        self.save_transaction.is_none(),
                        egui::Button::new("Open backup folder"),
                    )
                    .clicked()
                {
                    self.open_data_folder(&manager.backups);
                }
            });
            if dirty {
                muted(
                    ui,
                    "Save your pending settings changes before backing up settings or restoring.",
                );
            }
            match &self.backups {
                Ok(entries) if !entries.is_empty() => {
                    egui::ComboBox::from_id_salt("backup")
                        .selected_text(self.backup.as_deref().unwrap_or("Select backup"))
                        .show_ui(ui, |ui| {
                            for entry in entries {
                                ui.selectable_value(
                                    &mut self.backup,
                                    Some(entry.name.clone()),
                                    format!("{}  /  {}", entry.modified, entry.name),
                                );
                            }
                        });
                    egui::ComboBox::from_id_salt("restore scope")
                        .selected_text(scope_name(self.scope))
                        .show_ui(ui, |ui| {
                            for scope in [Scope::Saves, Scope::Settings, Scope::Both] {
                                if ui
                                    .selectable_label(
                                        scope_name(scope) == scope_name(self.scope),
                                        scope_name(scope),
                                    )
                                    .clicked()
                                {
                                    self.scope = scope;
                                }
                            }
                        });
                    if ui
                        .add_enabled(
                            !dirty && self.save_transaction.is_none(),
                            egui::Button::new("Restore selected backup..."),
                        )
                        .clicked()
                    {
                        if let Some(name) = self.backup.clone() {
                            self.update.defer_review();
                            self.restore = Some((name, self.scope));
                        }
                    }
                }
                Ok(_) => muted(ui, "No backups yet."),
                Err(error) => {
                    ui.colored_label(AMBER, format!("Could not read backups: {error}"));
                }
            }
        });
    }

    fn refresh_achievements(&mut self) {
        let result = (|| {
            let eboot = self
                .state
                .launch
                .args
                .first()
                .ok_or("No game dump configured.")?;
            let disc = game::locate(eboot).map_err(|e| e.to_string())?;
            trophy::read_catalog(&disc.root).map_err(|e| e.to_string())
        })();
        self.trophy_progress = match &result {
            Ok(catalog) => catalog
                .unlocked(&self.state.launch.cwd)
                .map_err(|e| e.to_string()),
            Err(_) => Ok(BTreeMap::new()),
        };
        self.trophy_catalog = Some(result);
    }

    fn achievements(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Achievements");
            if ui.button("Refresh progress").clicked() {
                self.refresh_achievements();
            }
        });
        let catalog = match &self.trophy_catalog {
            Some(Ok(catalog)) => catalog,
            Some(Err(error)) => {
                ui.colored_label(
                    AMBER,
                    format!("Could not read the disc's trophies: {error}"),
                );
                return;
            }
            None => return,
        };
        let unlocked = match &self.trophy_progress {
            Ok(unlocked) => unlocked,
            Err(error) => {
                ui.colored_label(
                    AMBER,
                    format!("Could not read local trophy progress: {error}"),
                );
                return;
            }
        };
        let total = catalog.trophies.len();
        let earned = catalog
            .trophies
            .iter()
            .filter(|t| unlocked.contains_key(&t.id))
            .count();
        card(ui, |ui| {
            ui.label(RichText::new(format!("{earned} of {total} trophies earned")).strong());
            ui.add(egui::ProgressBar::new(earned as f32 / total as f32));
        });
        card(ui, |ui| {
            for t in &catalog.trophies {
                let earned = unlocked.contains_key(&t.id);
                let (grade, color) = match t.grade {
                    'P' => ("Platinum", BLUE),
                    'G' => ("Gold", AMBER),
                    'S' => ("Silver", Color32::from_rgb(200, 217, 232)),
                    _ => ("Bronze", Color32::from_rgb(229, 159, 116)),
                };
                ui.horizontal_wrapped(|ui| {
                    ui.colored_label(color, grade);
                    ui.strong(if t.hidden && !earned {
                        "Hidden trophy"
                    } else {
                        &t.name
                    });
                    muted(ui, if earned { "Earned" } else { "Locked" });
                });
                if !t.hidden || earned {
                    if !t.detail.is_empty() {
                        muted(ui, t.detail.clone());
                    }
                }
                ui.separator();
            }
        });
    }

    fn start_update_check(&mut self, ctx: &egui::Context, manual: bool) {
        let root = self.state.launch.root.clone();
        let ctx = ctx.clone();
        let (send, completion) = mpsc::channel();
        let worker = std::thread::Builder::new().spawn(move || {
            let _ = send.send(update::preview(&root).map_err(|error| error.to_string()));
            ctx.request_repaint();
        });
        self.update = match worker {
            Ok(_) => UpdateState::Checking {
                completion,
                manual,
                open_review: manual,
            },
            Err(error) => UpdateState::Checked {
                result: Err(format!("Could not start the update check: {error}")),
                manual,
            },
        };
    }

    fn poll_update(&mut self, ctx: &egui::Context) {
        match &self.update {
            UpdateState::Checking {
                completion,
                manual,
                open_review,
            } => {
                let result = match completion.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        Err("The update check stopped unexpectedly.".into())
                    }
                    Err(mpsc::TryRecvError::Empty) => return,
                };
                self.update = match result {
                    Ok(preview)
                        if preview.available
                            && *open_review
                            && self.restore.is_none()
                            && self.save_transaction.is_none()
                            && !ctx.memory(|m| m.any_popup_open())
                            && !ctx.input(|i| i.viewport().close_requested()) =>
                    {
                        self.notice = None;
                        UpdateState::Reviewing(preview)
                    }
                    result => UpdateState::Checked {
                        result,
                        manual: *manual,
                    },
                };
            }
            UpdateState::Installing {
                installer,
                launcher,
            } => {
                let progress = installer.progress.lock().unwrap();
                match progress.done.as_ref() {
                    Some(Ok(())) => {
                        let launcher = launcher.clone();
                        drop(progress);
                        self.restart_launcher(ctx, launcher);
                    }
                    Some(Err(error)) => {
                        let error = error.clone();
                        let log = progress.log.join("\n");
                        drop(progress);
                        self.update = UpdateState::Failed { error, log };
                    }
                    None => {}
                }
            }
            _ => {}
        }
    }

    fn restart_launcher(&mut self, ctx: &egui::Context, launcher: PathBuf) {
        let result = Command::new(&launcher)
            .current_dir(&self.state.launch.root)
            .spawn();
        match result {
            Ok(_) => {
                self.update = UpdateState::Disabled;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(error) => {
                self.update = UpdateState::RestartFailed {
                    launcher,
                    error: error.to_string(),
                };
            }
        }
    }

    fn update_button(&mut self, ui: &mut egui::Ui) {
        if matches!(self.update, UpdateState::Disabled) {
            return;
        }
        let available =
            matches!(&self.update, UpdateState::Checked { result: Ok(info), .. } if info.available);
        let checking = matches!(self.update, UpdateState::Checking { .. });
        let label = if available {
            RichText::new("Update available").color(AMBER).strong()
        } else if checking {
            RichText::new("Checking for updates…")
        } else {
            RichText::new("Update")
        };
        if ui
            .add_enabled(
                !checking && self.save_transaction.is_none(),
                egui::Button::new(label),
            )
            .on_hover_text(if available {
                "Review what's new"
            } else {
                "Check for updates"
            })
            .clicked()
        {
            let prior = std::mem::replace(&mut self.update, UpdateState::Idle);
            match prior {
                UpdateState::Checked {
                    result: Ok(preview),
                    ..
                } if preview.available => {
                    self.notice = None;
                    self.update = UpdateState::Reviewing(preview);
                }
                _ => self.start_update_check(ui.ctx(), true),
            }
        }
    }

    fn update_status(&self, ui: &mut egui::Ui) {
        match &self.update {
            UpdateState::Checked {
                result: Ok(info),
                manual: true,
            } if !info.available => {
                muted(ui, format!("Up to date ({})", short_commit(&info.commit)));
            }
            UpdateState::Checked {
                result: Err(error),
                manual: true,
            } => {
                let message = format!("Could not check for updates. {error}");
                ui.add(egui::Label::new(RichText::new(&message).color(AMBER)).truncate())
                    .on_hover_text(message);
            }
            _ => {}
        }
    }

    fn show_update_window(&mut self, ctx: &egui::Context) {
        let window = |title| {
            egui::Window::new(title)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .resizable(false)
                .collapsible(false)
        };
        match &self.update {
            UpdateState::Reviewing(preview) => {
                let mut dismiss = false;
                let mut install = false;
                let mut save = false;
                window("What's New").show(ctx, |ui| {
                    ui.set_width(480.0);
                    muted(ui, format!("Update {}", short_commit(&preview.commit)));
                    if let Some(status) = &preview.notes_status {
                        muted(ui, status);
                    }
                    if !preview.changes.is_empty() {
                        egui::ScrollArea::vertical()
                            .max_height(260.0)
                            .show(ui, |ui| {
                                for change in &preview.changes {
                                    ui.label(format!("• {change}"));
                                }
                            });
                    } else if preview.notes_status.is_none() {
                        muted(ui, "Change notes are unavailable for this update.");
                    }
                    ui.separator();
                    if self.state.dirty() {
                        muted(
                            ui,
                            "Save your settings changes before installing the update.",
                        );
                        save = ui.button("Save settings").clicked();
                    }
                    if let Some((error, true)) = &self.notice {
                        ui.colored_label(AMBER, error);
                    }
                    ui.horizontal(|ui| {
                        dismiss = ui.button("Later").clicked();
                        let button = egui::Button::new(
                            RichText::new("Install Update").strong().color(BACKGROUND),
                        )
                        .fill(BLUE);
                        install = ui.add_enabled(!self.state.dirty(), button).clicked();
                    });
                });
                if dismiss
                    || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
                {
                    let prior = std::mem::replace(&mut self.update, UpdateState::Idle);
                    if let UpdateState::Reviewing(preview) = prior {
                        self.update = UpdateState::Checked {
                            result: Ok(preview),
                            manual: false,
                        };
                    }
                } else if install {
                    match std::env::current_exe() {
                        Ok(launcher) => {
                            let repaint = ctx.clone();
                            let installer = setup::Installer::start_launcher_update(
                                self.state.launch.root.clone(),
                                preview.commit.clone(),
                                Arc::new(move || repaint.request_repaint()),
                            );
                            self.update = UpdateState::Installing {
                                installer,
                                launcher,
                            };
                        }
                        Err(error) => {
                            self.notify(Err(format!("Could not locate the launcher: {error}")))
                        }
                    }
                } else if save {
                    let result = self.state.save().map(|()| "Settings saved.".into());
                    self.notify(result);
                }
            }
            UpdateState::Installing { installer, .. } => {
                let progress = installer.progress.lock().unwrap();
                window("Installing Update").show(ctx, |ui| {
                    ui.set_width(480.0);
                    ui.strong(if progress.phase.is_empty() {
                        "Starting…"
                    } else {
                        &progress.phase
                    });
                    ui.add(
                        egui::ProgressBar::new(progress.fraction)
                            .fill(BLUE)
                            .show_percentage(),
                    );
                    if !progress.detail.is_empty() {
                        muted(ui, &progress.detail);
                    }
                    muted(ui, "The launcher will restart when the update is complete.");
                });
            }
            UpdateState::Failed { error, log } => {
                let mut retry = false;
                let mut dismiss = false;
                window("Update Could Not Finish").show(ctx, |ui| {
                    ui.set_width(480.0);
                    ui.colored_label(AMBER, error);
                    ui.horizontal(|ui| {
                        dismiss = ui.button("Close").clicked();
                        retry = ui.button("Check again").clicked();
                        if !log.is_empty() && ui.button("Copy details").clicked() {
                            ctx.copy_text(log.clone());
                        }
                    });
                });
                if retry {
                    self.start_update_check(ctx, true);
                } else if dismiss {
                    self.update = UpdateState::Idle;
                }
            }
            UpdateState::RestartFailed { launcher, error } => {
                let mut restart = false;
                window("Update Installed").show(ctx, |ui| {
                    ui.set_width(480.0);
                    ui.colored_label(AMBER, format!("Could not restart the launcher. {error}"));
                    restart = ui.button("Restart launcher").clicked();
                });
                if restart {
                    self.restart_launcher(ctx, launcher.clone());
                }
            }
            _ => {}
        }
    }

    fn support(&mut self, ui: &mut egui::Ui) {
        ui.heading("Support");
        card(ui, |ui| {
            ui.label(RichText::new("Game logs").size(19.0).strong());
            let launch = &self.state.launch;
            let mut log = None;
            for (name, base) in [
                ("Output", launch.out.as_deref()),
                ("Errors", launch.err.as_deref()),
            ] {
                if let Some(base) = base {
                    let current = resolve(&launch.cwd, base);
                    let previous = current.with_extension("prev.log");
                    ui.horizontal_wrapped(|ui| {
                        for (label, path) in [
                            (format!("Current {name}"), current),
                            (format!("Previous {name}"), previous),
                        ] {
                            if ui
                                .add_enabled(path.is_file(), egui::Button::new(label))
                                .clicked()
                            {
                                log = Some(path);
                            }
                        }
                    });
                }
            }
            if let Some(path) = log {
                if let Err(e) = Command::new("notepad.exe").arg(path).spawn() {
                    self.notify(Err(format!("Could not open log: {e}")));
                }
            }
            if ui.button("Open log folder").clicked() {
                setup::open_folder(&self.state.launch.cwd);
            }
        });
        card(ui, |ui| {
            ui.label(RichText::new("Build & updates").size(19.0).strong());
            ui.label(self.region_label());
            if ui.button("Copy build info").clicked() {
                let root = &self.state.launch.root;
                let install = lock::State::load(&root.join(".installer/state"));
                let commit = std::fs::read_to_string(root.join(".installer/update.state"))
                    .ok()
                    .and_then(|t| {
                        t.lines()
                            .find_map(|l| l.strip_prefix("commit ").map(str::to_owned))
                    });
                let build =
                    exe_stamp(&self.state.launch.exe).unwrap_or_else(|| "Unavailable".into());
                let settings = DISPLAY
                    .iter()
                    .map(|r| format!("{}: {}", r.label, r.names[self.state.get(r)]))
                    .collect::<Vec<_>>()
                    .join("\n");
                ui.ctx().copy_text(format!(
                    "inFamous PC\nRegion: {}\nUpdater commit: {}\nGame executable (bytes and modified Unix time): {}\nEBOOT SHA-256: {}\n{settings}",
                    self.region_label(), commit.as_deref().unwrap_or("Unavailable"), build,
                    install.eboot.as_deref().unwrap_or("Unavailable")));
                self.notify(Ok("Build info copied.".into()));
            }
            ui.horizontal_wrapped(|ui| {
                ui.hyperlink_to(
                    "GitHub releases",
                    "https://github.com/WoofahRayetCode/inFAMOUS-US/releases",
                );
                ui.hyperlink_to(
                    "Report an issue",
                    "https://github.com/WoofahRayetCode/inFAMOUS-US/issues/new",
                );
            });
        });
        card(ui, |ui| {
            ui.label(RichText::new("Credits").size(19.0).strong());
            for (label, url) in CREDITS {
                ui.hyperlink_to(label, url);
            }
        });
    }
}

impl eframe::App for Dashboard {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        if let Some(features) = self.feature_check.as_ref().and_then(|r| r.try_recv().ok()) {
            self.state.set_features(features);
            self.feature_check = None;
        }
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        let was_running = self.save_transaction.is_some()
            || matches!(self.update, UpdateState::Installing { .. });
        if close_requested && was_running {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.poll_update(ctx);
        self.poll_save_transaction();
        egui::TopBottomPanel::top("tabs")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(22.0, 10.0)),
            )
            .show(ctx, |ui| {
                if self.restore.is_some() || self.update.blocks_dashboard() {
                    ui.disable();
                }
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("inFamous PC").strong().color(BLUE));
                    ui.separator();
                    for (tab, label) in [
                        (Tab::Play, "Play"),
                        (Tab::Settings, "Display & Controls"),
                        (Tab::Saves, "Saves"),
                        (Tab::Achievements, "Achievements"),
                        (Tab::Support, "Support"),
                    ] {
                        if ui.selectable_label(self.tab == tab, label).clicked() && self.tab != tab
                        {
                            self.update.defer_review();
                            self.tab = tab;
                            if tab == Tab::Saves && self.save_transaction.is_none() {
                                self.refresh_saves();
                            }
                            if tab == Tab::Achievements {
                                self.refresh_achievements();
                            }
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.update_button(ui);
                    });
                });
            });
        egui::TopBottomPanel::bottom("actions")
            .frame(
                egui::Frame::none()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(22.0, 11.0)),
            )
            .show(ctx, |ui| {
                if self.restore.is_some() || self.update.blocks_dashboard() {
                    ui.disable();
                }
                if let Some(transaction) = &self.save_transaction {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(transaction.progress);
                    });
                } else if let Some((text, error)) = &self.notice {
                    ui.colored_label(if *error { AMBER } else { BLUE }, text);
                }
                self.update_status(ui);
                ui.horizontal(|ui| {
                    muted(ui, self.summary());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let text = if self.state.launch.settings_only {
                            "Save & close"
                        } else {
                            "Play"
                        };
                        let play =
                            egui::Button::new(RichText::new(text).strong().color(BACKGROUND))
                                .fill(BLUE)
                                .min_size(egui::vec2(110.0, 33.0));
                        let idle = self.save_transaction.is_none();
                        if ui
                            .add_enabled(idle && self.readiness().is_ok(), play)
                            .clicked()
                        {
                            match self.state.play() {
                                Ok(()) => {
                                    self.played.store(true, Ordering::Relaxed);
                                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                Err(e) => self.notify(Err(e)),
                            }
                        }
                        if ui
                            .add_enabled(
                                idle && self.state.dirty(),
                                egui::Button::new("Save settings"),
                            )
                            .clicked()
                        {
                            let result = self.state.save().map(|()| "Settings saved.".into());
                            self.notify(result);
                        }
                    });
                });
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BACKGROUND)
                    .inner_margin(egui::Margin::symmetric(22.0, 16.0)),
            )
            .show(ctx, |ui| {
                ui.add_enabled_ui(
                    self.restore.is_none() && !self.update.blocks_dashboard(),
                    |ui| {
                        egui::ScrollArea::vertical()
                            .id_salt(match self.tab {
                                Tab::Play => "play",
                                Tab::Settings => "settings",
                                Tab::Saves => "saves",
                                Tab::Achievements => "achievements",
                                Tab::Support => "support",
                            })
                            .auto_shrink([false, false])
                            .show(ui, |ui| match self.tab {
                                Tab::Play => self.home(ui),
                                Tab::Settings => {
                                    ui.add_enabled_ui(self.save_transaction.is_none(), |ui| {
                                        self.settings(ui);
                                    });
                                }
                                Tab::Saves => self.saves(ui),
                                Tab::Achievements => self.achievements(ui),
                                Tab::Support => self.support(ui),
                            });
                    },
                );
            });
        if let Some((name, scope)) = self.restore.clone() {
            egui::Window::new("Restore backup")
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .resizable(false)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(format!("{} from {name}", scope_name(scope)));
                    ui.label("A recovery backup will be created before files are replaced.");
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.restore = None;
                        }
                        if ui
                            .add_enabled(!self.state.dirty(), egui::Button::new("Restore"))
                            .clicked()
                        {
                            self.restore = None;
                            self.start_save_transaction(ui.ctx(), Snapshot::Restore(name, scope));
                        }
                    });
                });
        }
        self.show_update_window(ctx);
        if close_requested
            && !was_running
            && (self.save_transaction.is_some()
                || matches!(self.update, UpdateState::Installing { .. }))
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
    }
}

fn banner_texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    let png = include_bytes!("../../../assets/launcher-skyline.png");
    let picture = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .into_rgb8();
    let size = [picture.width() as usize, picture.height() as usize];
    let color = egui::ColorImage::from_rgb(size, picture.as_raw());
    Some(ctx.load_texture("industrial waterfront", color, egui::TextureOptions::LINEAR))
}

pub(super) fn run(state: State) -> bool {
    let played = Arc::new(AtomicBool::new(false));
    let result = played.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 660.0])
            .with_min_inner_size([760.0, 530.0])
            .with_icon(egui::IconData {
                rgba: icon_rgba(64),
                width: 64,
                height: 64,
            }),
        centered: true,
        ..Default::default()
    };
    if let Err(error) = eframe::run_native(
        "inFamous PC",
        options,
        Box::new(move |cc| Ok(Box::new(Dashboard::new(state, result, &cc.egui_ctx)))),
    ) {
        message(&format!("Could not open the launcher: {error}"));
    }
    played.load(Ordering::Relaxed)
}
