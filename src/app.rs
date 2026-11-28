use chrono::{Datelike, Local, NaiveDate, Weekday};
use eframe::egui::{self, Align2, RichText, Stroke};
use opora::db;
use opora::entry::{self, Entry};
use rusqlite::Connection;

use crate::theme;

const MONTHS: [&str; 12] = [
    "января",
    "февраля",
    "марта",
    "апреля",
    "мая",
    "июня",
    "июля",
    "августа",
    "сентября",
    "октября",
    "ноября",
    "декабря",
];

/// Разделы приложения.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Sleep,
    Notes,
    Tasks,
    Goals,
    /// Пока пустой раздел: появится позже, но место в навигации уже есть.
    Settings,
}

impl Section {
    const ALL: [Section; 4] = [
        Section::Sleep,
        Section::Notes,
        Section::Tasks,
        Section::Goals,
    ];

    fn title(self) -> &'static str {
        match self {
            Section::Sleep => "Сон",
            Section::Notes => "Заметки",
            Section::Tasks => "Дела",
            Section::Goals => "Цели",
            Section::Settings => "Настройки",
        }
    }

    /// Ключ для таблицы `items`; у «Сна» и «Настроек» своих строк нет.
    fn key(self) -> &'static str {
        match self {
            Section::Sleep => "sleep",
            Section::Notes => "notes",
            Section::Tasks => "tasks",
            Section::Goals => "goals",
            Section::Settings => "settings",
        }
    }

    fn from_key(key: &str) -> Option<Self> {
        match key {
            "notes" => Some(Section::Notes),
            "tasks" => Some(Section::Tasks),
            "goals" => Some(Section::Goals),
            _ => None,
        }
    }
}

/// Строка в разделах «Заметки», «Дела», «Цели».
struct Item {
    id: i64,
    section: Section,
    /// Название — только у заметок.
    title: String,
    text: String,
    /// Отметка «сделано» — только у дел и целей.
    done: bool,
}

/// Заметка, открытая в полный размер. `editing == None` — новая.
#[derive(Default)]
struct NoteDraft {
    title: String,
    text: String,
    editing: Option<usize>,
}

/// Новое дело или цель до нажатия Ctrl+Enter.
struct TaskDraft {
    section: Section,
    text: String,
}

fn weekday_ru(day: NaiveDate) -> &'static str {
    match day.weekday() {
        Weekday::Mon => "понедельник",
        Weekday::Tue => "вторник",
        Weekday::Wed => "среда",
        Weekday::Thu => "четверг",
        Weekday::Fri => "пятница",
        Weekday::Sat => "суббота",
        Weekday::Sun => "воскресенье",
    }
}

pub struct App {
    conn: Connection,
    /// Открытый раздел.
    section: Section,
    /// Строки разделов «Заметки», «Дела», «Цели». Пока живут только в памяти.
    items: Vec<Item>,
    date: NaiveDate,
    sleep_text: String,
    mood: u8,
    /// Текст сновидений. «Не помню» хранится как пустая строка.
    note: String,
    /// Сновидение не запомнилось: поле скрыто, в базу уйдёт пустая строка.
    dream_unknown: bool,
    /// Заметка, открытая на весь экран. Пока одна на всё приложение.
    note_draft: Option<NoteDraft>,
    /// Новая строка дела/цели, которую ещё не создали.
    task_draft: Option<TaskDraft>,
    status: Option<String>,
    error: Option<String>,
    /// Лёгкое уведомление справа сверху: текст и момент, когда его гасить.
    toast: Option<(String, f64)>,
    /// Последние 30 дней — то, во что превращается нажатие «Сохранить».
    entries: Vec<Entry>,
    /// День, удаление которого ждёт подтверждения.
    pending_delete: Option<NaiveDate>,
    /// Только что добавленная строка ещё не получила курсор.
    focus_new: bool,
    /// Сколько кадров уже отрисовано — по нему ловится момент, когда окно показано.
    frames: u32,
}

impl App {
    pub fn new(conn: Connection, ctx: &egui::Context) -> Self {
        theme::install(ctx);
        // Стартовый масштаб — как после Ctrl+0, независимо от прошлой сессии.
        ctx.set_zoom_factor(1.0);

        let mut app = Self {
            conn,
            section: Section::Sleep,
            items: Vec::new(),
            date: entry::today(),
            sleep_text: String::new(),
            mood: 5,
            note: String::new(),
            dream_unknown: false,
            note_draft: None,
            task_draft: None,
            status: None,
            error: None,
            toast: None,
            entries: Vec::new(),
            pending_delete: None,
            focus_new: false,
            frames: 0,
        };
        app.load_day();
        app.reload_entries();
        app.reload_items();
        app
    }

    fn reload_entries(&mut self) {
        match db::recent(&self.conn, 30) {
            Ok(entries) => self.entries = entries,
            Err(err) => self.error = Some(format!("не удалось прочитать список: {err}")),
        }
    }

    fn reload_items(&mut self) {
        match db::items(&self.conn) {
            Ok(rows) => {
                self.items = rows
                    .into_iter()
                    .filter_map(|row| {
                        Some(Item {
                            id: row.id,
                            section: Section::from_key(&row.section)?,
                            title: row.title,
                            text: row.text,
                            done: row.done,
                        })
                    })
                    .collect();
            }
            Err(err) => self.error = Some(format!("не удалось прочитать заметки и дела: {err}")),
        }
    }

    fn load_day(&mut self) {
        match db::load(&self.conn, self.date) {
            Ok(Some(entry)) => {
                self.sleep_text = entry.sleep_hours.map(|h| h.to_string()).unwrap_or_default();
                self.mood = entry.mood;
                // Пустая заметка в базе и есть «не помню»: отдельного поля под
                // это нет, а дописывать схему ради одного флага незачем.
                self.dream_unknown = entry.note.trim().is_empty();
                self.note = entry.note;
                self.status = None;
            }
            Ok(None) => {
                self.sleep_text.clear();
                self.mood = 5;
                self.dream_unknown = false;
                self.note.clear();
                self.status = None;
            }
            Err(err) => self.error = Some(format!("не удалось прочитать запись: {err}")),
        }
    }

    fn parse_sleep(&self) -> Result<Option<f32>, String> {
        // Пустое поле — «не помню»: отдельный переключатель больше не нужен.
        let text = self.sleep_text.trim().replace(',', ".");
        if text.is_empty() {
            return Ok(None);
        }
        text.parse::<f32>()
            .map(Some)
            .map_err(|_| format!("сон: ожидается число часов, получено «{}»", self.sleep_text))
    }

    fn save(&mut self) {
        self.error = None;
        let note = if self.dream_unknown {
            ""
        } else {
            self.note.trim()
        };
        let result = self
            .parse_sleep()
            .and_then(|sleep| {
                Entry::new(self.date, sleep, self.mood, note).map_err(|e| e.to_string())
            })
            .and_then(|entry| db::save(&self.conn, &entry).map_err(|e| e.to_string()));

        match result {
            Ok(()) => {
                self.reload_entries();
                self.status = Some(format!("сохранено в {}", Local::now().format("%H:%M")));
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui) {
        let date = self.date;
        let mut add = false;

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(format!("{} {}", date.day(), MONTHS[date.month0() as usize]))
                        .font(theme::serif(34.0))
                        .color(theme::TEXT),
                );
                ui.label(
                    RichText::new(format!("{}, {}", weekday_ru(date), date.year()))
                        .font(theme::body(13.0))
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(6.0);
                // Название открытого раздела — под датой, крупно.
                ui.label(
                    RichText::new(self.section.title())
                        .font(theme::body(24.0))
                        .color(theme::TEXT),
                );
            });

            // Значок «новая запись» — там, где записи бывают, на уровне названия.
            // Пока открыт редактор, он не нужен: иначе можно потерять набранное.
            let editing = match self.section {
                Section::Notes => self.note_draft.is_some(),
                Section::Tasks | Section::Goals => self
                    .task_draft
                    .as_ref()
                    .is_some_and(|draft| draft.section == self.section),
                Section::Sleep | Section::Settings => false,
            };
            if matches!(
                self.section,
                Section::Notes | Section::Tasks | Section::Goals
            ) && !editing
            {
                let remaining = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(remaining, 104.0),
                    egui::Layout::right_to_left(egui::Align::BOTTOM),
                    |ui| {
                        if new_entry_icon(ui).clicked() {
                            add = true;
                        }
                    },
                );
            }
        });

        if add {
            match self.section {
                Section::Notes => self.note_draft = Some(NoteDraft::default()),
                Section::Tasks | Section::Goals => {
                    self.task_draft = Some(TaskDraft {
                        section: self.section,
                        text: String::new(),
                    });
                }
                Section::Sleep | Section::Settings => {}
            }
            self.focus_new = true;
        }
    }

    /// Разделы слева, вертикально: значок и подпись.
    fn nav(&mut self, ui: &mut egui::Ui) {
        ui.allocate_ui(egui::vec2(NAV_W, ui.available_height()), |ui| {
            ui.vertical(|ui| {
                ui.add_space(8.0);
                for section in Section::ALL {
                    self.nav_item(ui, section);
                    ui.add_space(NAV_GAP);
                }
                // Настройки — отдельно, у нижнего края панели.
                ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                    ui.add_space(8.0);
                    self.nav_item(ui, Section::Settings);
                });
            });
        });
    }

    fn nav_item(&mut self, ui: &mut egui::Ui, section: Section) {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(NAV_W, NAV_ITEM_H), egui::Sense::click());

        let selected = self.section == section;
        let color = if selected {
            theme::ACCENT
        } else if response.hovered() {
            theme::TEXT
        } else {
            theme::TEXT_DIM
        };
        if response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if response.clicked() && self.section != section {
            // Незаконченные строки не теряем при переходе.
            self.commit_task();
            self.finish_note();
            self.section = section;
        }

        // Значок и подпись — по одной вертикальной оси.
        let icon_rect = egui::Rect::from_min_size(
            egui::pos2(rect.center().x - 11.0, rect.top() + 6.0),
            egui::vec2(22.0, 22.0),
        );
        let painter = ui.painter();
        section_icon(painter, icon_rect, section, color);
        painter.text(
            egui::pos2(rect.center().x, rect.top() + 37.0),
            Align2::CENTER_TOP,
            section.title(),
            theme::body(15.0),
            color,
        );
    }

    /// Лёгкое уведомление справа сверху; гаснет само через пару секунд.
    fn show_toast(&mut self, ctx: &egui::Context) {
        let Some((message, until)) = self.toast.clone() else {
            return;
        };
        let now = ctx.input(|i| i.time);
        if now >= until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((until - now).max(0.01)));
        egui::Area::new(egui::Id::unique("toast"))
            .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-24.0, 18.0))
            .order(egui::Order::Foreground)
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::NONE
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::LINE))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(14, 9))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(&message)
                                .font(theme::body(13.5))
                                .color(theme::TEXT),
                        );
                    });
            });
    }

    /// Содержимое открытого раздела.
    fn page(&mut self, ui: &mut egui::Ui) {
        self.header(ui);
        ui.add_space(20.0);
        ui.separator();
        ui.add_space(24.0);

        if let Some(err) = &self.error {
            ui.label(
                RichText::new(err)
                    .font(theme::body(13.0))
                    .color(theme::ERROR),
            );
            ui.add_space(12.0);
        }

        match self.section {
            Section::Sleep => self.day_form(ui),
            Section::Notes => self.notes_grid(ui),
            Section::Tasks | Section::Goals => self.checklist_page(ui),
            // Настройки появятся позже; пока только место в навигации.
            Section::Settings => {}
        }
    }

    /// Дела и цели: номер, отметка и текст строки.
    fn checklist_page(&mut self, ui: &mut egui::Ui) {
        let section = self.section;
        let has_draft = self
            .task_draft
            .as_ref()
            .is_some_and(|draft| draft.section == section);

        // Ctrl+Enter создаёт набранное дело и сразу открывает следующую строку.
        // Событие забираем до полей, чтобы Enter не вставился в текст.
        if has_draft
            && ui.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Enter))
            && self.commit_task()
        {
            self.task_draft = Some(TaskDraft {
                section,
                text: String::new(),
            });
            // Пустая строка с тем же id: старый курсор сюда не переносим.
            let draft_id = egui::Id::unique(("task-draft", section.key()));
            egui::TextEdit::store_state(ui.ctx(), draft_id, Default::default());
            self.focus_new = true;
        }

        let mut any = false;
        // Курсор ставим в новую строку, а не в последнюю существующую.
        let focus_draft = std::mem::take(&mut self.focus_new);
        let count = self
            .items
            .iter()
            .filter(|item| item.section == section)
            .count();
        let hint = task_hint(section);

        for (index, item) in self
            .items
            .iter_mut()
            .filter(|item| item.section == section)
            .enumerate()
        {
            any = true;
            let number = index + 1;
            let mut changed = false;

            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2(26.0, 30.0),
                    egui::Label::new(
                        RichText::new(format!("{number}."))
                            .font(theme::body(15.0))
                            .color(theme::TEXT_DIM),
                    )
                    .halign(egui::Align::RIGHT),
                );

                if checkbox(ui, item.done).clicked() {
                    item.done = !item.done;
                    changed = true;
                }
                ui.add_space(6.0);

                let width = ui.available_width();
                let field_id = egui::Id::unique(("item-text", item.id));
                auto_close(ui, field_id, &mut item.text);
                let mut field = egui::TextEdit::singleline(&mut item.text)
                    .id(field_id)
                    .frame(egui::Frame::NONE)
                    .hint_text(hint)
                    .desired_width(width)
                    .min_size(egui::vec2(width, 30.0))
                    .font(theme::body(17.0));
                if item.done {
                    field = field.text_color(theme::TEXT_DIM);
                }
                let response = ui.add(field);
                underline(ui, &response);
                changed |= response.changed();
            });
            if changed {
                match db::update_item(&self.conn, item.id, &item.title, &item.text, item.done) {
                    Ok(()) => self.error = None,
                    Err(err) => {
                        self.error = Some(format!("не удалось сохранить дело: {err}"));
                    }
                }
            }
            ui.add_space(10.0);
        }

        // Новая строка живёт отдельно от списка, пока её не создали Ctrl+Enter.
        if let Some(draft) = self
            .task_draft
            .as_mut()
            .filter(|draft| draft.section == section)
        {
            any = true;
            let number = count + 1;
            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2(26.0, 30.0),
                    egui::Label::new(
                        RichText::new(format!("{number}."))
                            .font(theme::body(15.0))
                            .color(theme::TEXT_DIM),
                    )
                    .halign(egui::Align::RIGHT),
                );
                checkbox(ui, false);
                ui.add_space(6.0);

                let width = ui.available_width();
                let draft_id = egui::Id::unique(("task-draft", section.key()));
                auto_close(ui, draft_id, &mut draft.text);
                let response = ui.add(
                    egui::TextEdit::singleline(&mut draft.text)
                        .id(draft_id)
                        .frame(egui::Frame::NONE)
                        .hint_text(hint)
                        .desired_width(width)
                        .min_size(egui::vec2(width, 30.0))
                        .font(theme::body(17.0)),
                );
                underline(ui, &response);
                if focus_draft {
                    response.request_focus();
                }
            });
            ui.add_space(10.0);
        }

        if !any {
            ui.label(
                RichText::new("Пока пусто")
                    .font(theme::body(15.0))
                    .color(theme::TEXT_DIM),
            );
        }
    }

    /// Переносит набранное дело/цель в список.
    /// Сохраняет набранное дело или цель. Возвращает true, если строка создана.
    fn commit_task(&mut self) -> bool {
        self.error = None;
        let Some(draft) = self.task_draft.take() else {
            return false;
        };
        let text = draft.text.trim().to_string();
        if text.is_empty() {
            // Пустую строку не теряем: Ctrl+Enter на ней просто ничего не делает.
            self.task_draft = Some(draft);
            return false;
        }
        match db::insert_item(&self.conn, draft.section.key(), "", &text, false) {
            Ok(id) => {
                self.items.push(Item {
                    id,
                    section: draft.section,
                    title: String::new(),
                    text,
                    done: false,
                });
                true
            }
            Err(err) => {
                self.error = Some(format!("не удалось сохранить дело: {err}"));
                self.task_draft = Some(TaskDraft {
                    section: draft.section,
                    text,
                });
                false
            }
        }
    }

    /// Полноэкранный редактор заметки: «Сохранить» сверху слева, тело — на весь остаток.
    fn note_editor(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.set_width(ui.available_width());

            // Ctrl+Enter завершает заметку до того, как Enter попадёт в текст.
            if ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter)) {
                self.finish_note();
                return;
            }

            let mut save = false;
            let focus = std::mem::take(&mut self.focus_new);

            ui.horizontal(|ui| {
                if link(ui, "← к заметкам").clicked() {
                    save = true;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button = egui::Button::new(
                        RichText::new("Сохранить")
                            .font(theme::body(15.0))
                            .color(theme::BG),
                    )
                    .fill(theme::ACCENT)
                    .min_size(egui::vec2(150.0, 38.0));
                    if ui.add(button).clicked() {
                        save = true;
                    }
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Ctrl + Enter — готово")
                            .font(theme::body(12.5))
                            .color(theme::TEXT_DIM),
                    );
                });
            });
            ui.add_space(18.0);

            if let Some(err) = &self.error {
                ui.label(
                    RichText::new(err)
                        .font(theme::body(13.0))
                        .color(theme::ERROR),
                );
                ui.add_space(10.0);
            }

            let Some(draft) = self.note_draft.as_mut() else {
                return;
            };
            let width = ui.available_width();
            let title_id = egui::Id::unique("note-title");
            auto_close(ui, title_id, &mut draft.title);
            let title = ui.add(
                egui::TextEdit::singleline(&mut draft.title)
                    .id(title_id)
                    .frame(egui::Frame::NONE)
                    .hint_text("Название")
                    .desired_width(width)
                    .font(theme::serif(30.0)),
            );
            underline(ui, &title);
            if focus {
                title.request_focus();
            }
            ui.add_space(14.0);

            let field_height = (ui.available_height() - 18.0).max(160.0);
            let body_id = egui::Id::unique("note-body");
            // Enter обрабатываем до TextEdit: иначе маркер списка появляется
            // на кадр позже и курсор успевает мигнуть на старом месте.
            handle_enter(ui, body_id, &mut draft.text);
            let active_line = cursor_line(ui.ctx(), body_id, &draft.text);
            let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap_width: f32| {
                note_layouter(ui, text.as_str(), wrap_width, active_line)
            };
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, theme::LINE))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    auto_close(ui, body_id, &mut draft.text);
                    ui.add_sized(
                        egui::vec2(width - 22.0, field_height),
                        egui::TextEdit::multiline(&mut draft.text)
                            .id(body_id)
                            .frame(egui::Frame::NONE)
                            .layouter(&mut layouter),
                    );
                });

            if save {
                self.finish_note();
            }
        });
    }

    /// Сетка заметок: название и начало текста, клик открывает заметку.
    fn notes_grid(&mut self, ui: &mut egui::Ui) {
        let mut any = false;
        let mut open = None;
        let mut delete = None;
        let mut copied = false;

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(12.0, 12.0);
            for (index, item) in self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.section == Section::Notes)
            {
                any = true;
                let card = egui::Frame::NONE
                    .fill(theme::SURFACE)
                    .stroke(Stroke::new(1.0, theme::LINE))
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        // Фиксированный квадрат и явная вертикаль: иначе
                        // карточка центрирует содержимое и ставит текст в строку.
                        ui.allocate_ui_with_layout(
                            egui::vec2(NOTE_W - 24.0, NOTE_H - 24.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                let has_title = !item.title.trim().is_empty();
                                let title = if has_title {
                                    item.title.trim()
                                } else {
                                    "Без названия"
                                };
                                let color = if has_title {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_DIM
                                };
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(title).font(theme::body(16.0)).color(color),
                                    )
                                    .truncate(),
                                );
                                if !item.text.trim().is_empty() {
                                    ui.add_space(6.0);
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(note_preview(&item.text))
                                                .font(theme::body(14.0))
                                                .color(theme::TEXT_DIM),
                                        )
                                        .wrap(),
                                    );
                                }
                            },
                        );
                    });

                let rect = card.response.rect;
                let response = ui.interact(
                    rect,
                    ui.scope_id().with(("note", index)),
                    egui::Sense::click(),
                );
                if response.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    ui.painter().rect_stroke(
                        rect,
                        6.0,
                        Stroke::new(1.0, theme::TEXT_DIM),
                        egui::StrokeKind::Inside,
                    );
                }
                if response.clicked() {
                    open = Some(index);
                }
                // Правая кнопка сразу копирует заметку: одним нажатием.
                if response.clicked_by(egui::PointerButton::Secondary) {
                    ui.ctx().copy_text(note_clipboard_text(item));
                    copied = true;
                }
                response.context_menu(|ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 4.0);
                    if menu_action(ui, "Скопировать", theme::TEXT, MenuIcon::Copy) {
                        ui.ctx().copy_text(note_clipboard_text(item));
                        copied = true;
                        ui.close();
                    }
                    ui.separator();
                    if menu_action(ui, "Удалить", theme::ERROR, MenuIcon::Delete) {
                        delete = Some(index);
                        ui.close();
                    }
                });
            }
        });

        if copied {
            let until = ui.input(|i| i.time) + 2.0;
            self.toast = Some(("Скопировано в буфер".to_owned(), until));
        }
        if let Some(index) = delete {
            self.delete_note(index);
        }
        if let Some(index) = open {
            self.open_note(index);
        }
        if !any {
            ui.label(
                RichText::new("Пока пусто")
                    .font(theme::body(15.0))
                    .color(theme::TEXT_DIM),
            );
        }
    }

    fn delete_note(&mut self, index: usize) {
        self.error = None;
        let Some(item) = self.items.get(index) else {
            return;
        };
        match db::delete_item(&self.conn, item.id) {
            Ok(()) => {
                self.items.remove(index);
            }
            Err(err) => self.error = Some(format!("не удалось удалить заметку: {err}")),
        }
    }

    fn open_note(&mut self, index: usize) {
        let Some(item) = self.items.get(index) else {
            return;
        };
        self.note_draft = Some(NoteDraft {
            title: item.title.clone(),
            text: item.text.clone(),
            editing: Some(index),
        });
        self.focus_new = true;
    }

    /// Сохраняет открытую заметку в список и в базу. Пустую новую — не создаёт.
    fn finish_note(&mut self) {
        self.error = None;
        let Some(draft) = self.note_draft.take() else {
            return;
        };
        let title = draft.title.trim().to_string();
        let text = draft.text.trim_end().to_string();

        if let Some(index) = draft.editing {
            let Some(id) = self.items.get(index).map(|item| item.id) else {
                return;
            };
            match db::update_item(&self.conn, id, &title, &text, false) {
                Ok(()) => {
                    if let Some(item) = self.items.get_mut(index) {
                        item.title = title;
                        item.text = text;
                    }
                }
                Err(err) => {
                    self.error = Some(format!("не удалось сохранить заметку: {err}"));
                    self.note_draft = Some(draft);
                }
            }
        } else if !title.is_empty() || !text.trim().is_empty() {
            match db::insert_item(&self.conn, Section::Notes.key(), &title, &text, false) {
                Ok(id) => self.items.push(Item {
                    id,
                    section: Section::Notes,
                    title,
                    text,
                    done: false,
                }),
                Err(err) => {
                    self.error = Some(format!("не удалось сохранить заметку: {err}"));
                    self.note_draft = Some(draft);
                }
            }
        }
    }

    /// Форма дня и список последних дней — это раздел «Сон».
    fn day_form(&mut self, ui: &mut egui::Ui) {
        field_row(ui, "Настроение", |ui| self.mood_value(ui));
        ui.add_space(14.0);
        field_row(ui, "Сон", |ui| self.sleep_value(ui));
        ui.add_space(14.0);
        field_row_top(ui, "Сновидения", |ui| self.dream_value(ui));

        ui.add_space(36.0);
        ui.separator();
        ui.add_space(18.0);
        self.save_row(ui);

        ui.add_space(36.0);
        ui.separator();
        ui.add_space(14.0);
        self.history(ui);
    }

    fn mood_value(&mut self, ui: &mut egui::Ui) {
        mood_ruler(ui, &mut self.mood);
    }

    fn sleep_value(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.sleep_text)
                    .frame(egui::Frame::NONE)
                    .desired_width(46.0)
                    // Высота 30: клик по всей строке до линии, а не только по буквам.
                    .min_size(egui::vec2(46.0, 30.0))
                    .hint_text("7,5")
                    .font(theme::body(17.0)),
            );
            underline(ui, &response);

            ui.add_space(4.0);
            ui.label(
                RichText::new("часов")
                    .font(theme::body(13.0))
                    .color(theme::TEXT_DIM),
            );
        });
    }

    /// Сновидения: поле в рамке или «не помню» вместо него.
    fn dream_value(&mut self, ui: &mut egui::Ui) {
        if self.dream_unknown {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("Не помню")
                        .font(theme::body(17.0))
                        .color(theme::TEXT_DIM),
                );
                ui.add_space(12.0);
                if link(ui, "Помню").clicked() {
                    self.dream_unknown = false;
                }
            });
            return;
        }

        // Ссылка стоит рядом с полем, а не под ним: иначе «Сохранить» уезжает
        // за нижний край развёрнутого окна.
        let action = 64.0;
        let width = (ui.available_width() - action - 8.0).max(200.0);
        let dream_id = egui::Id::unique("dream-note");
        ui.horizontal_top(|ui| {
            egui::Frame::NONE
                .stroke(Stroke::new(1.0, theme::LINE))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    auto_close(ui, dream_id, &mut self.note);
                    ui.add_sized(
                        egui::vec2(width - 22.0, 88.0),
                        egui::TextEdit::multiline(&mut self.note)
                            .id(dream_id)
                            .frame(egui::Frame::NONE)
                            .font(theme::body(17.0)),
                    );
                });
            ui.add_space(4.0);
            if link(ui, "Не помню").clicked() {
                self.dream_unknown = true;
            }
        });
    }

    fn save_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let button = egui::Button::new(
                RichText::new("Сохранить")
                    .font(theme::body(15.0))
                    .color(theme::BG),
            )
            .fill(theme::ACCENT)
            .min_size(egui::vec2(160.0, 38.0));

            if ui.add(button).clicked() {
                self.save();
            }

            if let Some(status) = &self.status {
                ui.add_space(8.0);
                ui.label(
                    RichText::new(status)
                        .font(theme::body(13.0))
                        .color(theme::TEXT_DIM),
                );
            }
        });
    }

    /// Последние дни под формой: сохранённая запись сразу видна здесь.
    fn history(&mut self, ui: &mut egui::Ui) {
        if self.entries.is_empty() {
            ui.label(
                RichText::new("пока нет записей")
                    .font(theme::body(13.0))
                    .color(theme::TEXT_DIM),
            );
            return;
        }

        let mut ask: Option<NaiveDate> = None;
        let mut confirmed: Option<NaiveDate> = None;
        let mut cancel = false;

        for entry in &self.entries {
            let date = entry.date;
            let pending = self.pending_delete == Some(date);

            ui.horizontal(|ui| {
                ui.add_sized(
                    egui::vec2(56.0, 26.0),
                    egui::Label::new(
                        RichText::new(date.format("%d.%m").to_string())
                            .font(theme::body(13.0))
                            .color(theme::TEXT_DIM),
                    )
                    .halign(egui::Align::LEFT),
                );
                ui.add_sized(
                    egui::vec2(84.0, 26.0),
                    egui::Label::new(
                        RichText::new(metrics(entry))
                            .font(theme::body(13.0))
                            .color(theme::TEXT_DIM),
                    )
                    .halign(egui::Align::LEFT),
                );

                // Место под «удалить» занято всегда, иначе заметка дёргается
                // при каждом наведении.
                let note_width = (ui.available_width() - DELETE_W).max(60.0);
                ui.add_sized(
                    egui::vec2(note_width, 26.0),
                    egui::Label::new(
                        RichText::new(&entry.note)
                            .font(theme::body(15.0))
                            .color(theme::TEXT),
                    )
                    .truncate(),
                );

                if link(ui, "удалить").clicked() {
                    ask = Some(date);
                }
            });

            // Подтверждение стоит справа, под ссылкой «удалить»: отвечать там же,
            // где нажимал, а не искать кнопки в другом конце строки. Прижимаем
            // его к строке: отступ сетки 12 pt здесь читается как дыра.
            if pending {
                ui.add_space(-10.0);
                let width = ui.available_width();
                ui.allocate_ui_with_layout(
                    egui::vec2(width, 18.0),
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        // Кнопки-ссылки без вертикальных полей: иначе строка
                        // подтверждения вырастает до высоты обычной кнопки.
                        ui.spacing_mut().button_padding = egui::vec2(6.0, 0.0);
                        if link(ui, "да").clicked() {
                            confirmed = Some(date);
                        }
                        if link(ui, "нет").clicked() {
                            cancel = true;
                        }
                        ui.label(
                            RichText::new(format!("удалить запись за {}?", date.format("%d.%m")))
                                .font(theme::body(13.0))
                                .color(theme::TEXT_DIM),
                        );
                    },
                );
            }

            ui.add_space(6.0);
        }

        if cancel {
            self.pending_delete = None;
        }
        if let Some(date) = ask {
            self.pending_delete = Some(date);
        }
        if let Some(date) = confirmed {
            self.delete_day(date);
        }
    }

    /// Удаляет запись за день. Восстановить её будет нечем.
    fn delete_day(&mut self, date: NaiveDate) {
        match db::delete(&self.conn, date) {
            Ok(_) => {
                self.pending_delete = None;
                if date == self.date {
                    self.load_day();
                }
                self.reload_entries();
                self.status = Some(format!("удалено: {}", date.format("%d.%m")));
            }
            Err(err) => self.error = Some(format!("не удалось удалить: {err}")),
        }
    }
}

/// Линия под полем — вместо рамки, как строка в бумажном дневнике.
/// В фокусе она акцентная: видно, куда встал курсор.
fn underline(ui: &egui::Ui, response: &egui::Response) {
    let (width, color) = if response.has_focus() {
        (1.5, theme::ACCENT)
    } else {
        (1.0, theme::LINE)
    };
    let rect = response.rect;
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(width, color),
    );
}

/// Кнопка «сделано»: залитый квадрат с галочкой, а не пустые скобки.
fn checkbox(ui: &mut egui::Ui, done: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let box_rect = egui::Rect::from_center_size(rect.center(), egui::vec2(18.0, 18.0));
    let painter = ui.painter();
    if done {
        painter.rect_filled(box_rect, 5.0, theme::ACCENT);
        painter.add(egui::Shape::line(
            vec![
                egui::pos2(box_rect.left() + 4.5, box_rect.center().y + 0.5),
                egui::pos2(box_rect.center().x - 0.5, box_rect.bottom() - 4.5),
                egui::pos2(box_rect.right() - 4.0, box_rect.top() + 4.8),
            ],
            Stroke::new(1.8, theme::BG),
        ));
    } else {
        let (fill, border) = if response.hovered() {
            (theme::SURFACE_HOVER, theme::TEXT_DIM)
        } else {
            (theme::SURFACE, theme::LINE)
        };
        painter.rect_filled(box_rect, 5.0, fill);
        painter.rect_stroke(
            box_rect,
            5.0,
            Stroke::new(1.2, border),
            egui::StrokeKind::Inside,
        );
    }

    response
}

/// Значок «новая запись»: лист и карандаш внутри него.
fn new_entry_icon(ui: &mut egui::Ui) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(26.0, 26.0), egui::Sense::click());
    let color = if response.hovered() {
        theme::TEXT
    } else {
        theme::TEXT_DIM
    };
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let painter = ui.painter();
    let sheet = rect.shrink(4.0);
    painter.rect_stroke(
        sheet,
        3.0,
        Stroke::new(1.3, color),
        egui::StrokeKind::Inside,
    );

    // Карандаш целиком внутри листа, с отступом от рамки: снаружи он
    // читался как перечёркнутый квадрат.
    let margin = 5.5;
    // Грифель смотрит в нижний левый угол — туда, куда пишут.
    let tail = egui::pos2(sheet.right() - margin, sheet.top() + margin);
    let tip = egui::pos2(sheet.left() + margin, sheet.bottom() - margin);
    let dir = (tip - tail).normalized();
    let normal = egui::vec2(-dir.y, dir.x) * 1.5;
    let body_end = tip - dir * 2.6;
    painter.add(egui::Shape::convex_polygon(
        vec![
            tail + normal,
            body_end + normal,
            tip,
            body_end - normal,
            tail - normal,
        ],
        color,
        Stroke::NONE,
    ));

    response
}

/// Иконки разделов: месяц, лист, галочка, мишень.
fn section_icon(painter: &egui::Painter, rect: egui::Rect, section: Section, color: egui::Color32) {
    let stroke = Stroke::new(1.4, color);
    match section {
        Section::Sleep => {
            let radius = rect.width() * 0.44;
            painter.circle_filled(rect.center(), radius, color);
            // Вырезаем сдвинутый круг фоном — получается месяц.
            painter.circle_filled(
                rect.center() + egui::vec2(radius * 0.55, -radius * 0.45),
                radius * 0.85,
                theme::BG,
            );
        }
        Section::Notes => {
            let sheet = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 4.0, rect.top() + 2.0),
                egui::pos2(rect.right() - 4.0, rect.bottom() - 2.0),
            );
            painter.rect_stroke(sheet, 2.0, stroke, egui::StrokeKind::Inside);
            for line in 0..3 {
                let y = sheet.top() + 5.5 + line as f32 * 4.2;
                painter.line_segment(
                    [
                        egui::pos2(sheet.left() + 3.5, y),
                        egui::pos2(sheet.right() - 3.5, y),
                    ],
                    Stroke::new(1.1, color),
                );
            }
        }
        Section::Tasks => {
            let box_rect = rect.shrink(3.0);
            painter.rect_stroke(box_rect, 3.0, stroke, egui::StrokeKind::Inside);
            painter.add(egui::Shape::line(
                vec![
                    egui::pos2(box_rect.left() + 4.0, box_rect.center().y + 0.5),
                    egui::pos2(box_rect.center().x - 0.5, box_rect.bottom() - 4.0),
                    egui::pos2(box_rect.right() - 3.5, box_rect.top() + 5.0),
                ],
                Stroke::new(1.6, color),
            ));
        }
        Section::Goals => {
            painter.circle_stroke(rect.center(), rect.width() * 0.45, stroke);
            painter.circle_stroke(rect.center(), rect.width() * 0.24, stroke);
            painter.circle_filled(rect.center(), 1.7, color);
        }
        Section::Settings => {
            let center = rect.center();
            let radius = rect.width() * 0.28;
            painter.circle_stroke(center, radius, stroke);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::TAU / 8.0;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment(
                    [
                        center + direction * (radius + 1.0),
                        center + direction * (radius + 4.0),
                    ],
                    Stroke::new(1.8, color),
                );
            }
            painter.circle_filled(center, 1.8, color);
        }
    }
}

/// Текстовая ссылка: без рамки, с линией под курсором.
fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let response = ui.add(
        egui::Button::new(
            RichText::new(text)
                .font(theme::body(13.0))
                .color(theme::TEXT_DIM),
        )
        .frame(false),
    );
    if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        let rect = response.rect;
        ui.painter().line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, theme::TEXT_DIM),
        );
    }
    response
}

#[derive(Clone, Copy)]
enum MenuIcon {
    Copy,
    Delete,
}

fn note_clipboard_text(item: &Item) -> String {
    let title = item.title.trim();
    let text = item.text.trim_end();
    if title.is_empty() {
        text.to_string()
    } else if text.is_empty() {
        title.to_string()
    } else {
        format!("{title}\n\n{text}")
    }
}

/// Пункт контекстного меню с иконкой: без стандартной кнопки, чтобы
/// управлять цветом удаления и отступом иконки.
fn menu_action(ui: &mut egui::Ui, label: &str, color: egui::Color32, icon: MenuIcon) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(190.0, 28.0), egui::Sense::click());
    let background = if response.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        ui.painter()
            .rect_filled(rect.shrink(2.0), 4.0, theme::SURFACE_HOVER);
        theme::SURFACE_HOVER
    } else {
        ui.visuals().window_fill
    };

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 16.0, rect.center().y),
        egui::vec2(16.0, 16.0),
    );
    draw_menu_icon(ui.painter(), icon_rect, icon, color, background);
    ui.painter().text(
        egui::pos2(rect.left() + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        theme::body(14.0),
        color,
    );

    response.clicked()
}

fn draw_menu_icon(
    painter: &egui::Painter,
    rect: egui::Rect,
    icon: MenuIcon,
    color: egui::Color32,
    background: egui::Color32,
) {
    let stroke = Stroke::new(1.3, color);
    match icon {
        MenuIcon::Copy => {
            let back =
                egui::Rect::from_min_size(rect.min + egui::vec2(2.0, 2.0), egui::vec2(9.0, 9.0));
            painter.rect_stroke(back, 1.5, stroke, egui::StrokeKind::Inside);
            let front =
                egui::Rect::from_min_size(rect.min + egui::vec2(5.0, 5.0), egui::vec2(9.0, 9.0));
            painter.rect_filled(front, 1.5, background);
            painter.rect_stroke(front, 1.5, stroke, egui::StrokeKind::Inside);
        }
        MenuIcon::Delete => {
            let lid = rect.top() + 3.0;
            painter.line_segment(
                [
                    egui::pos2(rect.left() + 3.0, lid),
                    egui::pos2(rect.right() - 3.0, lid),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    egui::pos2(rect.center().x - 2.0, lid - 2.0),
                    egui::pos2(rect.center().x + 2.0, lid - 2.0),
                ],
                stroke,
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(rect.left() + 4.0, lid + 3.0),
                egui::pos2(rect.right() - 4.0, rect.bottom() - 2.0),
            );
            painter.rect_stroke(body, 2.0, stroke, egui::StrokeKind::Inside);
            for x in [rect.center().x - 2.0, rect.center().x + 2.0] {
                painter.line_segment(
                    [egui::pos2(x, lid + 6.0), egui::pos2(x, rect.bottom() - 5.0)],
                    Stroke::new(1.0, color),
                );
            }
        }
    }
}

/// Настроение и сон одной строкой: «7/10 · 7,5 ч».
fn metrics(entry: &Entry) -> String {
    let mood = format!("{}/10", entry.mood);
    match entry.sleep_hours {
        // f32 Display сам убирает хвостовой ноль: 7.0 -> «7».
        Some(hours) => format!("{mood} · {} ч", hours.to_string().replace('.', ",")),
        None => mood,
    }
}

/// Подсказка в пустом поле: у дел и целей она разная.
fn task_hint(section: Section) -> &'static str {
    if section == Section::Goals {
        "Цель"
    } else {
        "Дело"
    }
}

/// Начало текста для карточки заметки: несколько строк, дальше многоточие.
fn note_preview(text: &str) -> String {
    let mut chars = text.chars();
    let mut preview: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        preview.push('…');
    }
    // Пустые строки тоже считаются: иначе карточка может вырасти на весь экран.
    if preview.lines().count() > 6 {
        preview = preview.lines().take(6).collect::<Vec<_>>().join("\n");
        preview.push('…');
    }
    preview
}

#[derive(Clone, Copy)]
enum ListMarker {
    Dash,
    Number { value: u64, digits: usize },
}

/// Разобранное начало строки списка: отступ, маркер и где кончается префикс.
struct ListPrefix {
    indent: String,
    marker: ListMarker,
    content_start: usize,
    content_empty: bool,
}

fn list_prefix(line: &str) -> Option<ListPrefix> {
    let indent: String = line
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();
    let rest = &line[indent.len()..];

    let marker = if let Some(after) = rest.strip_prefix('-') {
        if !after.is_empty() && !after.starts_with(' ') && !after.starts_with('\t') {
            return None;
        }
        ListMarker::Dash
    } else {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.is_empty() || !rest[digits.len()..].starts_with('.') {
            return None;
        }
        ListMarker::Number {
            value: digits.parse().unwrap_or(0),
            digits: digits.len(),
        }
    };

    let mut content_start = indent.len()
        + match marker {
            ListMarker::Dash => 1,
            ListMarker::Number { digits, .. } => digits + 1,
        };
    if line[content_start..].starts_with(' ') || line[content_start..].starts_with('\t') {
        content_start += 1;
    }

    Some(ListPrefix {
        indent,
        marker,
        content_start,
        content_empty: line[content_start..].trim().is_empty(),
    })
}

/// Enter на строке списка продолжает его: «1.» → «2.», «-» → «-».
/// На пустом пункте Enter убирает маркер и оставляет пустую строку.
fn continue_list(text: &mut String, ctx: &egui::Context, id: egui::Id) {
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let cursor = range.primary.index.0;
    let chars: Vec<char> = text.chars().collect();
    // Курсор должен стоять сразу после только что вставленного перевода строки.
    if cursor == 0 || chars.get(cursor - 1) != Some(&'\n') {
        return;
    }

    let prev_end = cursor - 1;
    let prev_start = chars[..prev_end]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1);
    let prev: String = chars[prev_start..prev_end].iter().collect();
    let Some(prefix) = list_prefix(&prev) else {
        return;
    };

    if prefix.content_empty {
        // Пустой пункт не размножаем: стираем его вместе с переводом строки.
        let mut new_chars = chars;
        new_chars.drain(prev_start..cursor);
        *text = new_chars.into_iter().collect();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(prev_start),
            )));
        state.store(ctx, id);
        return;
    }

    let marker = match prefix.marker {
        ListMarker::Dash => format!("{}- ", prefix.indent),
        ListMarker::Number { value, .. } => {
            format!("{}{}. ", prefix.indent, value.saturating_add(1))
        }
    };

    let marker_chars: Vec<char> = marker.chars().collect();
    let new_cursor = cursor + marker_chars.len();
    let mut new_chars = chars;
    new_chars.splice(cursor..cursor, marker_chars);
    *text = new_chars.into_iter().collect();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(new_cursor),
        )));
    state.store(ctx, id);
}

fn text_width(ui: &egui::Ui, text: &str, font: egui::FontId) -> f32 {
    let job = egui::text::LayoutJob::simple(text.to_owned(), font, theme::TEXT, f32::INFINITY);
    ui.fonts_mut(|fonts| fonts.layout_job(job).rect.width())
}

/// Начало заголовка: сколько символов спрятать и каким кеглем рисовать текст.
fn heading_prefix(line: &str) -> Option<(usize, f32)> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if !(1..=6).contains(&hashes) || !line[hashes..].starts_with(' ') {
        return None;
    }
    let size = match hashes {
        1 => 28.0,
        2 => 25.0,
        3 => 22.0,
        4 => 20.0,
        5 => 18.0,
        _ => 17.0,
    };
    Some((hashes + 1, size))
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "---" || trimmed == "***" || trimmed == "___"
}

/// Сколько символов занимает отступ: маркер списка или ведущие пробелы.
fn prefix_end(line: &str) -> usize {
    list_prefix(line)
        .map(|prefix| prefix.content_start)
        .unwrap_or_else(|| line.chars().take_while(|c| *c == ' ' || *c == '\t').count())
}

/// Строка с курсором: её рисуем как обычный текст, иначе при наборе «#»
/// строка меняет кегль, и курсор прыгает под новую раскладку.
fn cursor_line(ctx: &egui::Context, id: egui::Id, text: &str) -> Option<usize> {
    if !ctx.memory(|memory| memory.has_focus(id)) {
        return None;
    }
    let cursor = egui::TextEdit::load_state(ctx, id)?
        .cursor
        .char_range()?
        .primary
        .index
        .0;
    Some(text.chars().take(cursor).filter(|c| *c == '\n').count())
}

/// Раскладка заметки: заголовки, горизонтальные линии и висячий отступ списков.
fn note_layouter(
    ui: &egui::Ui,
    text: &str,
    wrap_width: f32,
    active_line: Option<usize>,
) -> std::sync::Arc<egui::Galley> {
    let font = theme::body(17.0);
    let color = theme::TEXT;
    let mut galleys = Vec::new();

    for (index, line) in text.split('\n').enumerate() {
        // Строку под курсором рисуем без разметки: заголовок становится
        // крупным только когда курсор уходит на следующую строку.
        let editing = active_line == Some(index);
        if !editing && let Some((prefix, size)) = heading_prefix(line) {
            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = wrap_width;
            job.keep_trailing_whitespace = true;
            job.append(
                &line[..prefix],
                0.0,
                egui::text::TextFormat::simple(theme::body(1.0), egui::Color32::TRANSPARENT),
            );
            job.append(
                &line[prefix..],
                0.0,
                egui::text::TextFormat::simple(theme::serif(size), color),
            );
            let galley = (*ui.fonts_mut(|fonts| fonts.layout_job(job))).clone();
            galleys.push(std::sync::Arc::new(galley));
            continue;
        }

        if !editing && is_rule(line) {
            let natural = text_width(ui, line, font.clone());
            let gaps = line.chars().count().saturating_sub(1).max(1) as f32;
            let extra = ((wrap_width - natural) / gaps).max(0.0);
            let mut format = egui::text::TextFormat::simple(font.clone(), theme::BG);
            format.strikethrough = Stroke::new(1.0, theme::LINE);
            format.line_height = Some(20.0);
            format.extra_letter_spacing = extra;

            let mut job = egui::text::LayoutJob::default();
            job.wrap.max_width = wrap_width;
            job.keep_trailing_whitespace = true;
            job.append(line, 0.0, format);
            let galley = (*ui.fonts_mut(|fonts| fonts.layout_job(job))).clone();
            galleys.push(std::sync::Arc::new(galley));
            continue;
        }

        let prefix = prefix_end(line);
        if prefix == 0 {
            let mut job =
                egui::text::LayoutJob::simple(line.to_owned(), font.clone(), color, wrap_width);
            job.keep_trailing_whitespace = true;
            let galley = (*ui.fonts_mut(|fonts| fonts.layout_job(job))).clone();
            galleys.push(std::sync::Arc::new(galley));
            continue;
        }

        // Маркер списка бледный, как в прежнем просмотре; переносы строк
        // выравниваем по тексту, а не по маркеру. Пункты списка чуть
        // просторнее обычных строк — как было в просмотре.
        let is_list = list_prefix(line).is_some();
        let line_height = is_list.then_some(26.0);
        let prefix_text = &line[..prefix];
        let prefix_chars = prefix_text.chars().count();
        let marker_color = if is_list { theme::TEXT_DIM } else { color };
        let measured = text_width(ui, prefix_text, font.clone());
        let mut job = egui::text::LayoutJob::default();
        job.wrap.max_width = (wrap_width - measured).max(1.0);
        job.keep_trailing_whitespace = true;
        let mut marker_format = egui::text::TextFormat::simple(font.clone(), marker_color);
        marker_format.line_height = line_height;
        job.append(prefix_text, 0.0, marker_format);
        let mut text_format = egui::text::TextFormat::simple(font.clone(), color);
        text_format.line_height = line_height;
        job.append(&line[prefix..], 0.0, text_format);
        let mut galley = (*ui.fonts_mut(|fonts| fonts.layout_job(job))).clone();

        // Сдвиг берём не из измерения префикса, а из реальной позиции первого
        // символа текста: так перенос совпадает с ним с точностью до пикселя.
        let indent = galley
            .rows
            .first()
            .and_then(|row| row.glyphs.get(prefix_chars))
            .map_or(measured, |glyph| glyph.pos.x);
        for placed in galley.rows.iter_mut().skip(1) {
            let Some(first) = placed
                .row
                .glyphs
                .iter()
                .position(|glyph| !glyph.chr.is_whitespace())
            else {
                continue;
            };
            placed.pos.x = indent - placed.row.glyphs[first].pos.x;
        }
        galleys.push(std::sync::Arc::new(galley));
    }

    let job = egui::text::LayoutJob::simple(text.to_owned(), font, color, wrap_width);
    std::sync::Arc::new(egui::Galley::concat(
        std::sync::Arc::new(job),
        &galleys,
        ui.ctx().pixels_per_point(),
    ))
}

/// Забирает Enter из очереди событий: его обрабатываем сами, до TextEdit.
fn take_enter(ui: &mut egui::Ui) -> Option<bool> {
    let mut shift = None;
    ui.input_mut(|input| {
        input.events.retain(|event| {
            if let egui::Event::Key {
                key: egui::Key::Enter,
                pressed: true,
                modifiers,
                ..
            } = event
                && !modifiers.command
                && !modifiers.alt
            {
                if shift.is_none() {
                    shift = Some(modifiers.shift);
                }
                false
            } else {
                true
            }
        });
    });
    shift
}

/// Enter в заметке: новый пункт списка, мягкий перенос или пустая строка
/// после заголовка. Всё делаем до TextEdit, чтобы не было мигания курсора.
fn handle_enter(ui: &mut egui::Ui, id: egui::Id, text: &mut String) {
    if !ui.memory(|memory| memory.has_focus(id)) {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    if range.primary.index != range.secondary.index {
        return;
    }
    let Some(shift) = take_enter(ui) else {
        return;
    };

    let chars: Vec<char> = text.chars().collect();
    let cursor = range.primary.index.0.min(chars.len());
    let at_line_end = chars[cursor..]
        .iter()
        .position(|&c| c == '\n')
        .is_none_or(|offset| offset == 0);

    let mut new_chars = chars;
    new_chars.insert(cursor, '\n');
    *text = new_chars.into_iter().collect();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor + 1),
        )));
    state.store(ui.ctx(), id);

    if shift {
        // Мягкий перенос с отступом до текста пункта.
        soft_line_indent(ui, id, text);
    } else {
        continue_list(text, ui.ctx(), id);
        if at_line_end {
            gap_after_heading(text, ui.ctx(), id);
        }
    }
}

/// После заголовка оставляем пустую строку — если следующая строка не заголовок.
fn gap_after_heading(text: &mut String, ctx: &egui::Context, id: egui::Id) {
    let Some(mut state) = egui::TextEdit::load_state(ctx, id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let cursor = range.primary.index.0;
    let chars: Vec<char> = text.chars().collect();
    if cursor == 0 || chars.get(cursor - 1) != Some(&'\n') {
        return;
    }

    let prev_end = cursor - 1;
    let prev_start = chars[..prev_end]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1);
    let prev: String = chars[prev_start..prev_end].iter().collect();
    if heading_prefix(&prev).is_none() {
        return;
    }

    let line_end = chars[cursor..]
        .iter()
        .position(|&c| c == '\n')
        .map_or(chars.len(), |i| cursor + i);
    let line: String = chars[cursor..line_end].iter().collect();
    // Подряд идущие заголовки стоят без пустой строки.
    if heading_prefix(&line).is_some() {
        return;
    }
    // Пустая строка уже есть — курсор просто стоит в ней.
    if line.trim().is_empty() && line_end < chars.len() {
        return;
    }

    let mut new_chars = chars;
    new_chars.insert(cursor, '\n');
    *text = new_chars.into_iter().collect();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor + 1),
        )));
    state.store(ctx, id);
}

/// Shift+Enter: перенос с отступом до текста списка.
fn soft_line_indent(ui: &egui::Ui, id: egui::Id, text: &mut String) {
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    let cursor = range.primary.index.0;
    let chars: Vec<char> = text.chars().collect();
    if cursor == 0 || chars.get(cursor - 1) != Some(&'\n') {
        return;
    }

    let prev_end = cursor - 1;
    let prev_start = chars[..prev_end]
        .iter()
        .rposition(|&c| c == '\n')
        .map_or(0, |i| i + 1);
    let prev: String = chars[prev_start..prev_end].iter().collect();
    let prefix = prefix_end(&prev);
    if prefix == 0 {
        return;
    }

    let font = theme::body(17.0);
    let prefix_width = text_width(ui, &prev[..prefix], font.clone());
    let space_width = text_width(ui, " ", font);
    if space_width <= 0.0 {
        return;
    }
    let spaces = (prefix_width / space_width).round() as usize;
    if spaces == 0 {
        return;
    }

    let mut new_chars = chars;
    new_chars.splice(cursor..cursor, std::iter::repeat_n(' ', spaces));
    *text = new_chars.into_iter().collect();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor + spaces),
        )));
    state.store(ui.ctx(), id);
}

/// Закрывающий символ для открывающей скобки или кавычки.
fn pair_for(open: char) -> Option<char> {
    match open {
        '(' => Some(')'),
        '[' => Some(']'),
        '{' => Some('}'),
        '"' => Some('"'),
        '«' => Some('»'),
        _ => None,
    }
}

/// Пара для оборачивания выделения: годится и закрывающая скобка.
fn wrap_pair(ch: char) -> Option<(char, char)> {
    match ch {
        '(' | ')' => Some(('(', ')')),
        '[' | ']' => Some(('[', ']')),
        '{' | '}' => Some(('{', '}')),
        '"' => Some(('"', '"')),
        '«' | '»' => Some(('«', '»')),
        _ => None,
    }
}

/// Кавычка сразу после слова — закрывающая, вторую не ставим.
fn closing_quote(chars: &[char], cursor: usize) -> bool {
    cursor
        .checked_sub(1)
        .and_then(|index| chars.get(index))
        .is_some_and(|prev| prev.is_alphanumeric())
}

/// До вставки текста в поле дописывает парную скобку или кавычку.
/// События забираем у поля заранее: иначе при быстрой печати пара
/// вставляется в конец текста, а не под курсор.
fn auto_close(ui: &mut egui::Ui, id: egui::Id, text: &mut String) {
    if !ui.memory(|memory| memory.has_focus(id)) {
        return;
    }
    let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) else {
        return;
    };
    let Some(range) = state.cursor.char_range() else {
        return;
    };
    // Поле могло получить другой текст с тем же id: курсор из прошлого
    // состояния бывает дальше конца строки.
    let len = text.chars().count();
    let primary = range.primary.index.0.min(len);
    let secondary = range.secondary.index.0.min(len);
    if primary != secondary {
        let start = primary.min(secondary);
        let end = primary.max(secondary);

        let events = ui.input_mut(|input| std::mem::take(&mut input.events));
        let mut kept = Vec::new();
        let mut wrapped = None;
        for event in events {
            if wrapped.is_none()
                && let egui::Event::Text(value) = &event
                && value.chars().count() == 1
                && let Some(pair) = value.chars().next().and_then(wrap_pair)
            {
                wrapped = Some(pair);
            } else {
                kept.push(event);
            }
        }
        ui.input_mut(|input| input.events = kept);

        let Some((open, close)) = wrapped else {
            return;
        };
        let chars: Vec<char> = text.chars().collect();
        let mut new_chars = Vec::with_capacity(chars.len() + 2);
        new_chars.extend_from_slice(&chars[..start]);
        new_chars.push(open);
        new_chars.extend_from_slice(&chars[start..end]);
        new_chars.push(close);
        new_chars.extend_from_slice(&chars[end..]);
        *text = new_chars.into_iter().collect();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(end + 2),
            )));
        state.store(ui.ctx(), id);
        return;
    }

    let cursor = primary;
    let mut chars: Vec<char> = text.chars().collect();

    // Backspace внутри пустой пары стирает обе скобки: `("")` → `""`.
    if cursor > 0
        && let Some(close) = pair_for(chars[cursor - 1])
        && chars.get(cursor) == Some(&close)
    {
        let pressed = ui.input_mut(|input| {
            let before = input.events.len();
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::Key {
                        key: egui::Key::Backspace,
                        pressed: true,
                        ..
                    }
                )
            });
            before != input.events.len()
        });
        if pressed {
            chars.remove(cursor);
            chars.remove(cursor - 1);
            *text = chars.into_iter().collect();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(cursor - 1),
                )));
            state.store(ui.ctx(), id);
            return;
        }
    }

    let events = ui.input_mut(|input| std::mem::take(&mut input.events));
    let mut kept = Vec::new();
    let mut cursor = cursor;
    let mut changed = false;

    for event in events {
        let egui::Event::Text(value) = &event else {
            kept.push(event);
            continue;
        };
        let mut symbols = value.chars();
        let Some(ch) = symbols.next() else {
            kept.push(event);
            continue;
        };
        if symbols.next().is_some() {
            // Вставка нескольких символов — не наше дело.
            kept.push(event);
            continue;
        }

        if matches!(ch, ')' | ']' | '}' | '"' | '»') && chars.get(cursor) == Some(&ch) {
            // Пользователь сам набрал закрывающую — перешагиваем авто-пару.
            cursor += 1;
            changed = true;
        } else if let Some(close) = pair_for(ch) {
            if ch == '"' && closing_quote(&chars, cursor) {
                kept.push(event);
            } else {
                chars.insert(cursor, ch);
                chars.insert(cursor + 1, close);
                cursor += 1;
                changed = true;
            }
        } else {
            kept.push(event);
        }
    }
    ui.input_mut(|input| input.events = kept);

    if !changed {
        return;
    }

    *text = chars.into_iter().collect();
    state
        .cursor
        .set_char_range(Some(egui::text::CCursorRange::one(
            egui::text::CCursor::new(cursor),
        )));
    state.store(ui.ctx(), id);
}

/// Ширина колонки контента: как страница, а не форма на весь экран.
const COLUMN: f32 = 620.0;
/// Ширина левой навигации: по самой длинной подписи, без запаса справа.
const NAV_W: f32 = 64.0;
/// Высота пункта навигации: значок сверху, подпись снизу.
const NAV_ITEM_H: f32 = 56.0;
/// Просвет между пунктами: подписи не должны слипаться.
const NAV_GAP: f32 = 28.0;
/// Поля окна: слева меньше, чтобы значки стояли ближе к краю.
const WINDOW_MARGIN: egui::Margin = egui::Margin {
    left: 20,
    right: 32,
    top: 32,
    bottom: 32,
};
/// Заметка — квадрат: сторона и внутренние отступы.
const NOTE_W: f32 = 186.0;
const NOTE_H: f32 = 186.0;
/// Ширина колонки подписей — по ней выровнены все строки формы.
const LABEL_W: f32 = 110.0;
/// Место под ссылку «удалить» в строке списка.
const DELETE_W: f32 = 64.0;

/// Строка формы: подпись фиксированной ширины, поле занимает остаток колонки.
fn field_row(ui: &mut egui::Ui, caption: &str, field: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(LABEL_W, 30.0), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            Align2::LEFT_CENTER,
            caption,
            theme::body(13.0),
            theme::TEXT_DIM,
        );
        field(ui);
    });
}

/// Та же строка, но подпись у верхней кромки — для высоких полей.
fn field_row_top(ui: &mut egui::Ui, caption: &str, field: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_top(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(LABEL_W, 30.0), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            Align2::LEFT_CENTER,
            caption,
            theme::body(13.0),
            theme::TEXT_DIM,
        );
        field(ui);
    });
}

/// Шкала 1–10: десять делений, выбранное залито акцентом.
fn mood_ruler(ui: &mut egui::Ui, mood: &mut u8) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for value in 1..=10u8 {
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(26.0, 30.0), egui::Sense::click());

            let selected = *mood == value;
            if selected {
                ui.painter()
                    .rect_filled(rect.shrink(1.0), 4.0, theme::ACCENT);
            } else if response.hovered() {
                ui.painter()
                    .rect_filled(rect.shrink(1.0), 4.0, theme::SURFACE);
            }

            let color = if selected {
                theme::BG
            } else if response.hovered() {
                theme::TEXT
            } else {
                theme::TEXT_DIM
            };
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                value.to_string(),
                theme::body(13.5),
                color,
            );

            if response.clicked() {
                *mood = value;
            }
            if response.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
        }
    });
}

/// Красит заголовок окна в цвет фона приложения: DWM — единственный способ
/// задать цвет неклиентской области в Windows 11.
#[cfg(windows)]
fn tint_title_bar(frame: &eframe::Frame) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Graphics::Dwm::{
        DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR, DwmSetWindowAttribute,
    };

    let Ok(handle) = frame.window_handle() else {
        return;
    };
    let RawWindowHandle::Win32(window) = handle.as_raw() else {
        return;
    };

    // COLORREF — 0x00BBGGRR.
    let caption: u32 = 0x001A_1617; // BG   #17161A
    let text: u32 = 0x00DD_E7ED; // TEXT #EDE7DD

    let hwnd = window.hwnd.get() as *mut core::ffi::c_void;
    #[expect(unsafe_code)]
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_CAPTION_COLOR as u32,
            &raw const caption as *const core::ffi::c_void,
            size_of::<u32>() as u32,
        );
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TEXT_COLOR as u32,
            &raw const text as *const core::ffi::c_void,
            size_of::<u32>() as u32,
        );
    }
}

#[cfg(not(windows))]
fn tint_title_bar(_frame: &eframe::Frame) {}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        // Окно появляется только после первого кадра, а «развернуть» скрытое окно
        // под Windows ненадёжно: с with_maximized на этой машине окно уезжает
        // за экран. Поэтому разворачиваем на втором кадре, когда оно уже видно.
        self.frames += 1;
        if self.frames == 1 {
            tint_title_bar(frame);
        }
        if self.frames == 2 && !ui.ctx().input(|i| i.viewport().maximized.unwrap_or(false)) {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Maximized(true));
        }

        self.show_toast(ui.ctx());

        egui::Frame::NONE
            .fill(theme::BG)
            .inner_margin(WINDOW_MARGIN)
            .show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    self.nav(ui);
                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    // Заметка занимает весь остаток экрана, но навигация слева
                    // остаётся: раздел видно, из него можно уйти.
                    if self.section == Section::Notes && self.note_draft.is_some() {
                        self.note_editor(ui);
                        return;
                    }

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            // Колонка не шире 620 и никогда не шире остатка окна:
                            // на узком окне она сжимается, а не уезжает за край.
                            let column = ui.available_width().min(COLUMN);
                            let offset = ((ui.available_width() - column) / 2.0).max(0.0);

                            ui.horizontal_top(|ui| {
                                ui.add_space(offset);
                                ui.vertical(|ui| {
                                    ui.set_width(column);
                                    self.page(ui);
                                });
                            });
                        });
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app(path: &std::path::Path) -> App {
        App {
            conn: db::open(path).unwrap(),
            section: Section::Sleep,
            items: Vec::new(),
            date: entry::today(),
            sleep_text: String::new(),
            mood: 5,
            note: String::new(),
            dream_unknown: false,
            note_draft: None,
            task_draft: None,
            status: None,
            error: None,
            toast: None,
            entries: Vec::new(),
            pending_delete: None,
            focus_new: false,
            frames: 0,
        }
    }

    #[test]
    fn save_writes_day_to_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = test_app(&path);

        app.mood = 7;
        app.sleep_text = "8,5".to_string();
        app.note = "тест формы".to_string();
        app.save();

        assert!(app.error.is_none(), "ошибка: {:?}", app.error);

        let check = db::open(&path).unwrap();
        let saved = db::load(&check, app.date).unwrap().unwrap();
        assert_eq!(saved.mood, 7);
        assert_eq!(saved.sleep_hours, Some(8.5));
        assert_eq!(saved.note, "тест формы");
    }

    #[test]
    fn saved_note_appears_in_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(&dir.path().join("opora.db"));

        app.note = "запись дня".to_string();
        app.save();

        assert!(app.error.is_none(), "ошибка: {:?}", app.error);
        assert_eq!(app.entries.len(), 1);
        assert_eq!(app.entries[0].date, app.date);
        assert_eq!(app.entries[0].note, "запись дня");
    }

    #[test]
    fn delete_clears_day_and_list() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(&dir.path().join("opora.db"));

        app.mood = 8;
        app.note = "удалить меня".to_string();
        app.save();
        assert_eq!(app.entries.len(), 1);

        let day = app.date;
        app.delete_day(day);

        assert!(app.error.is_none(), "ошибка: {:?}", app.error);
        assert!(app.entries.is_empty(), "список должен опустеть");
        assert_eq!(app.mood, 5, "форма возвращается к пустому дню");
        assert!(app.status.is_some());
    }

    #[test]
    fn bad_sleep_text_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(&dir.path().join("opora.db"));

        app.sleep_text = "много".to_string();
        app.save();

        assert!(app.status.is_none());
        assert!(app.error.is_some());
    }

    #[test]
    fn dream_unknown_saves_empty_note_and_survives_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = test_app(&path);

        app.dream_unknown = true;
        app.note = "это не должно сохраниться".to_string();
        app.save();
        assert!(app.error.is_none(), "ошибка: {:?}", app.error);

        let check = db::open(&path).unwrap();
        let saved = db::load(&check, app.date).unwrap().unwrap();
        assert_eq!(saved.note, "");

        app.note.clear();
        app.load_day();
        assert!(app.dream_unknown, "пустая заметка снова скрывает поле");
    }

    #[test]
    fn note_is_created_only_after_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = test_app(&path);

        app.note_draft = Some(NoteDraft {
            title: " идея ".to_string(),
            text: "текст заметки".to_string(),
            editing: None,
        });
        app.finish_note();

        assert!(app.note_draft.is_none());
        assert_eq!(app.items.len(), 1);
        assert!(app.items[0].section == Section::Notes);
        assert_eq!(app.items[0].title, "идея");
        assert_eq!(app.items[0].text, "текст заметки");

        let check = db::open(&path).unwrap();
        let rows = db::items(&check).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].section, "notes");
        assert_eq!(rows[0].title, "идея");
        assert_eq!(rows[0].text, "текст заметки");
    }

    #[test]
    fn empty_note_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(&dir.path().join("opora.db"));

        app.note_draft = Some(NoteDraft::default());
        app.finish_note();

        assert!(app.items.is_empty());
    }

    #[test]
    fn existing_note_is_updated_on_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = test_app(&path);

        app.note_draft = Some(NoteDraft {
            title: "старое".to_string(),
            text: "старый текст".to_string(),
            editing: None,
        });
        app.finish_note();
        app.open_note(0);
        let draft = app.note_draft.as_mut().unwrap();
        draft.title = "новое".to_string();
        draft.text = "новый текст".to_string();
        app.finish_note();

        assert_eq!(app.items.len(), 1);
        assert_eq!(app.items[0].title, "новое");
        assert_eq!(app.items[0].text, "новый текст");

        let check = db::open(&path).unwrap();
        let rows = db::items(&check).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "новое");
        assert_eq!(rows[0].text, "новый текст");
    }

    #[test]
    fn task_is_created_on_commit() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = test_app(&path);

        app.task_draft = Some(TaskDraft {
            section: Section::Tasks,
            text: " купить хлеб ".to_string(),
        });
        app.commit_task();

        assert!(app.task_draft.is_none());
        assert_eq!(app.items.len(), 1);
        assert!(app.items[0].section == Section::Tasks);
        assert_eq!(app.items[0].text, "купить хлеб");

        let check = db::open(&path).unwrap();
        let rows = db::items(&check).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].section, "tasks");
        assert_eq!(rows[0].text, "купить хлеб");
        assert!(!rows[0].done);
    }

    fn run_continue(text: &mut String, cursor: usize) {
        let ctx = egui::Context::default();
        let id = egui::Id::unique("continue-list-test");
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        egui::TextEdit::store_state(&ctx, id, state);
        continue_list(text, &ctx, id);
    }

    #[test]
    fn numbered_list_continues_on_enter() {
        let mut text = "1. первое\n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "1. первое\n2. ");

        let mut text = "  3. пункт\n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "  3. пункт\n  4. ");
    }

    #[test]
    fn dash_list_continues_on_enter() {
        let mut text = "- пункт\n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "- пункт\n- ");
    }

    #[test]
    fn plain_line_is_not_a_list() {
        let mut text = "обычная строка\n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "обычная строка\n");

        let mut text = "1) не тот маркер\n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "1) не тот маркер\n");
    }

    #[test]
    fn second_enter_removes_empty_list_item() {
        let mut text = "1. первое\n2. \n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "1. первое\n");

        let mut text = "- \n".to_string();
        let end = text.chars().count();
        run_continue(&mut text, end);
        assert_eq!(text, "");
    }

    #[test]
    fn list_prefix_parses_markers() {
        let prefix = list_prefix("  10. пункт").unwrap();
        assert_eq!(prefix.indent, "  ");
        assert_eq!(prefix.content_start, 6);
        assert!(matches!(
            prefix.marker,
            ListMarker::Number { value: 10, .. }
        ));
        assert!(!prefix.content_empty);

        let prefix = list_prefix("- пункт").unwrap();
        assert_eq!(prefix.content_start, 2);
        assert!(matches!(prefix.marker, ListMarker::Dash));

        assert!(list_prefix("-пункт").is_none());
        assert!(list_prefix("---").is_none());
        assert!(list_prefix("1) пункт").is_none());
    }

    #[test]
    fn list_wraps_align_with_text_not_marker() {
        let mut galley = None;
        egui::__run_test_ui(|ui| {
            galley = Some(note_layouter(
                ui,
                "1. Очень длинный пункт списка, который должен переноситься на следующую строку и выравниваться по тексту, а не под цифрой.",
                300.0,
                None,
            ));
        });
        let galley = galley.unwrap();
        assert!(galley.rows.len() >= 2, "строка должна перенестись");
        let text_x = galley.rows[0].glyphs[3].pos.x;
        let row = &galley.rows[1];
        let first = row
            .glyphs
            .iter()
            .find(|glyph| !glyph.chr.is_whitespace())
            .unwrap();
        let wrapped_x = row.pos.x + first.pos.x;
        assert!(text_x > 5.0, "маркер должен занимать место: {text_x}");
        assert!(
            (text_x - wrapped_x).abs() < 1.0,
            "перенос {wrapped_x} должен совпасть с текстом {text_x}"
        );
    }

    #[test]
    fn heading_is_plain_on_the_cursor_line() {
        let mut heights = (0.0, 0.0);
        egui::__run_test_ui(|ui| {
            let heading = note_layouter(ui, "# Заголовок", 300.0, None);
            let editing = note_layouter(ui, "# Заголовок", 300.0, Some(0));
            heights = (heading.rows[0].size.y, editing.rows[0].size.y);
        });
        assert!(
            heights.0 > heights.1 + 3.0,
            "заголовок {} должен быть крупнее строки под курсором {}",
            heights.0,
            heights.1
        );
    }

    #[test]
    fn selection_is_wrapped_in_pairs() {
        fn wrap(initial: &str, start: usize, end: usize, typed: &str) -> String {
            let ctx = egui::Context::default();
            ctx.set_fonts(egui::FontDefinitions::empty());
            let mut text = initial.to_owned();
            let body_id = egui::Id::unique("wrap-selection");
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(body_id));
                response.request_focus();
            });
            output.drop_without_applying_deltas();
            let mut state = egui::TextEdit::load_state(&ctx, body_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::two(
                    egui::text::CCursor::new(start),
                    egui::text::CCursor::new(end),
                )));
            state.store(&ctx, body_id);

            let input = egui::RawInput {
                events: vec![egui::Event::Text(typed.to_owned())],
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                auto_close(ui, body_id, &mut text);
                ui.add(egui::TextEdit::singleline(&mut text).id(body_id));
            });
            output.drop_without_applying_deltas();
            text
        }

        assert_eq!(wrap("текст", 0, 5, "("), "(текст)");
        assert_eq!(wrap("текст", 1, 4, ")"), "т(екс)т");
        assert_eq!(wrap("текст", 0, 5, "\""), "\"текст\"");
    }

    #[test]
    fn brackets_and_quotes_close_automatically() {
        fn typed(events: &[&str]) -> String {
            let ctx = egui::Context::default();
            ctx.set_fonts(egui::FontDefinitions::empty());
            let mut text = String::new();
            let body_id = egui::Id::unique("body");
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let response = ui.add(egui::TextEdit::singleline(&mut text).id(body_id));
                response.request_focus();
            });
            output.drop_without_applying_deltas();
            let mut state = egui::TextEdit::load_state(&ctx, body_id).unwrap_or_default();
            state
                .cursor
                .set_char_range(Some(egui::text::CCursorRange::one(
                    egui::text::CCursor::new(0),
                )));
            state.store(&ctx, body_id);

            for event in events {
                let input = egui::RawInput {
                    events: vec![egui::Event::Text((*event).to_owned())],
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| {
                    auto_close(ui, body_id, &mut text);
                    ui.add(egui::TextEdit::singleline(&mut text).id(body_id));
                });
                output.drop_without_applying_deltas();
            }
            text
        }

        assert_eq!(typed(&["("]), "()");
        assert_eq!(typed(&["(", "т", "е", "с", "т", ")"]), "(тест)");
        assert_eq!(typed(&["\""]), "\"\"");
        assert_eq!(typed(&["\"", "т", "е", "с", "т", "\""]), "\"тест\"");
        assert_eq!(typed(&["["]), "[]");
        assert_eq!(typed(&["{"]), "{}");
        assert_eq!(typed(&["«"]), "«»");
    }

    /// Набирает один символ в поле с курсором в конце текста.
    fn type_at_end(initial: &str, typed: &str) -> String {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let mut text = initial.to_owned();
        let id = egui::Id::unique("type-at-end");
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
            response.request_focus();
        });
        output.drop_without_applying_deltas();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        let end = text.chars().count();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(end),
            )));
        state.store(&ctx, id);

        let input = egui::RawInput {
            events: vec![egui::Event::Text(typed.to_owned())],
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            auto_close(ui, id, &mut text);
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        });
        output.drop_without_applying_deltas();
        text
    }

    #[test]
    fn quote_after_word_is_not_doubled() {
        assert_eq!(type_at_end("слово", "\""), "слово\"");
        assert_eq!(type_at_end("", "\""), "\"\"");
        assert_eq!(type_at_end("слово ", "\""), "слово \"\"");
    }

    #[test]
    fn stale_cursor_does_not_panic() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let mut text = String::new();
        let id = egui::Id::unique("stale-cursor");
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
            response.request_focus();
        });
        output.drop_without_applying_deltas();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(10),
            )));
        state.store(&ctx, id);

        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            auto_close(ui, id, &mut text);
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        });
        output.drop_without_applying_deltas();
        assert_eq!(text, "");
    }

    #[test]
    fn backspace_removes_an_empty_pair() {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let mut text = "\"\"".to_owned();
        let id = egui::Id::unique("backspace-pair");
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.add(egui::TextEdit::singleline(&mut text).id(id));
            response.request_focus();
        });
        output.drop_without_applying_deltas();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(1),
            )));
        state.store(&ctx, id);

        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Backspace,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            auto_close(ui, id, &mut text);
            ui.add(egui::TextEdit::singleline(&mut text).id(id));
        });
        output.drop_without_applying_deltas();
        assert_eq!(text, "");
    }

    fn run_enter(initial: &str, cursor: usize) -> (String, usize) {
        let ctx = egui::Context::default();
        ctx.set_fonts(egui::FontDefinitions::empty());
        let mut text = initial.to_owned();
        let id = egui::Id::unique("enter-test");
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let response = ui.add(egui::TextEdit::multiline(&mut text).id(id));
            response.request_focus();
        });
        output.drop_without_applying_deltas();
        let mut state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(cursor),
            )));
        state.store(&ctx, id);

        let input = egui::RawInput {
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| {
            handle_enter(ui, id, &mut text);
            ui.add(egui::TextEdit::multiline(&mut text).id(id));
        });
        output.drop_without_applying_deltas();
        let state = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        let cursor = state
            .cursor
            .char_range()
            .map_or(0, |range| range.primary.index.0);
        (text, cursor)
    }

    #[test]
    fn heading_gets_a_blank_line_after_enter() {
        let (text, cursor) = run_enter("# Заголовок", "# Заголовок".chars().count());
        assert_eq!(text, "# Заголовок\n\n");
        assert_eq!(cursor, text.chars().count());
    }

    #[test]
    fn next_heading_gets_no_extra_blank_line() {
        let (text, _) = run_enter("# А\n## Б", "# А".chars().count());
        assert_eq!(text, "# А\n\n## Б");
    }

    #[test]
    fn list_item_gets_no_blank_line() {
        let (text, cursor) = run_enter("1. пункт", "1. пункт".chars().count());
        assert_eq!(text, "1. пункт\n2. ");
        assert_eq!(cursor, text.chars().count());
    }

    #[test]
    fn clipboard_text_has_title_and_body() {
        let item = Item {
            id: 1,
            section: Section::Notes,
            title: "Идея".to_string(),
            text: "Текст заметки".to_string(),
            done: false,
        };
        assert_eq!(
            note_clipboard_text(&item),
            "Идея\n\nТекст заметки".to_string()
        );
    }
}
