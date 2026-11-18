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
    date: NaiveDate,
    sleep_text: String,
    sleep_unknown: bool,
    mood: u8,
    note: String,
    status: Option<String>,
    error: Option<String>,
}

impl App {
    pub fn new(conn: Connection, ctx: &egui::Context) -> Self {
        theme::install(ctx);

        let mut app = Self {
            conn,
            date: entry::today(),
            sleep_text: String::new(),
            sleep_unknown: false,
            mood: 5,
            note: String::new(),
            status: None,
            error: None,
        };
        app.load_day();
        app
    }

    fn load_day(&mut self) {
        match db::load(&self.conn, self.date) {
            Ok(Some(entry)) => {
                self.sleep_unknown = entry.sleep_hours.is_none();
                self.sleep_text = entry.sleep_hours.map(|h| h.to_string()).unwrap_or_default();
                self.mood = entry.mood;
                self.note = entry.note;
                self.status = None;
            }
            Ok(None) => {
                self.sleep_unknown = false;
                self.sleep_text.clear();
                self.mood = 5;
                self.note.clear();
                self.status = None;
            }
            Err(err) => self.error = Some(format!("не удалось прочитать запись: {err}")),
        }
    }

    fn parse_sleep(&self) -> Result<Option<f32>, String> {
        if self.sleep_unknown {
            return Ok(None);
        }
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
        let result = self
            .parse_sleep()
            .and_then(|sleep| {
                Entry::new(self.date, sleep, self.mood, self.note.trim()).map_err(|e| e.to_string())
            })
            .and_then(|entry| db::save(&self.conn, &entry).map_err(|e| e.to_string()));

        match result {
            Ok(()) => {
                self.status = Some(format!("сохранено в {}", Local::now().format("%H:%M")));
            }
            Err(err) => self.error = Some(err),
        }
    }

    fn header(&self, ui: &mut egui::Ui) {
        let date = self.date;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("{} {}", date.day(), MONTHS[date.month0() as usize]))
                    .font(theme::serif(34.0))
                    .color(theme::TEXT),
            );

            let remaining = ui.available_width();
            ui.allocate_ui_with_layout(
                egui::vec2(remaining, 40.0),
                egui::Layout::right_to_left(egui::Align::BOTTOM),
                |ui| {
                    ui.label(
                        RichText::new(format!("{}, {}", weekday_ru(date), date.year()))
                            .font(theme::body(13.0))
                            .color(theme::TEXT_DIM),
                    );
                },
            );
        });
    }

    fn mood_value(&mut self, ui: &mut egui::Ui) {
        mood_ruler(ui, &mut self.mood);
    }

    fn sleep_value(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if self.sleep_unknown {
                ui.label(
                    RichText::new("не помню")
                        .font(theme::body(17.0))
                        .color(theme::TEXT_DIM),
                );
            } else {
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.sleep_text)
                        .frame(egui::Frame::NONE)
                        .desired_width(46.0)
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
            }

            let toggle = if self.sleep_unknown {
                "помню"
            } else {
                "не помню"
            };
            let response = ui.add(
                egui::Button::new(
                    RichText::new(toggle)
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
            if response.clicked() {
                self.sleep_unknown = !self.sleep_unknown;
            }
        });
    }

    fn note_value(&mut self, ui: &mut egui::Ui) {
        let width = ui.available_width();
        let response = ui.add(
            egui::TextEdit::singleline(&mut self.note)
                .frame(egui::Frame::NONE)
                .hint_text("одна строка")
                .desired_width(width)
                .font(theme::body(17.0)),
        );
        underline(ui, &response);
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
}

/// Линия под полем — вместо рамки, как строка в бумажном дневнике.
fn underline(ui: &egui::Ui, response: &egui::Response) {
    let rect = response.rect;
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0, theme::LINE),
    );
}

fn label(text: &str) -> RichText {
    RichText::new(text)
        .font(theme::body(13.0))
        .color(theme::TEXT_DIM)
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

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::NONE
            .fill(theme::BG)
            .inner_margin(egui::Margin::same(32))
            .show(ui, |ui| {
                // Колонка фиксированной ширины по центру окна — как страница,
                // а не растянутая форма на весь экран.
                let column = 620.0;
                let offset = ((ui.available_width() - column) / 2.0).max(0.0);

                ui.horizontal(|ui| {
                    ui.add_space(offset);
                    ui.vertical(|ui| {
                        ui.set_width(column);
                        ui.set_max_width(column);

                        self.header(ui);
                        ui.add_space(20.0);
                        ui.separator();
                        ui.add_space(24.0);

                        egui::Grid::new("day")
                            .num_columns(2)
                            .min_col_width(110.0)
                            .max_col_width(column - 110.0 - 24.0)
                            .spacing([24.0, 22.0])
                            .show(ui, |ui| {
                                ui.label(label("настроение"));
                                self.mood_value(ui);
                                ui.end_row();

                                ui.label(label("сон"));
                                self.sleep_value(ui);
                                ui.end_row();

                                ui.label(label("что было"));
                                self.note_value(ui);
                                ui.end_row();
                            });

                        if let Some(err) = &self.error {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(err)
                                    .font(theme::body(13.0))
                                    .color(egui::Color32::from_rgb(0xD0, 0x6C, 0x5E)),
                            );
                        }

                        ui.add_space(36.0);
                        ui.separator();
                        ui.add_space(18.0);
                        self.save_row(ui);
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
            date: entry::today(),
            sleep_text: String::new(),
            sleep_unknown: false,
            mood: 5,
            note: String::new(),
            status: None,
            error: None,
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
    fn bad_sleep_text_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let mut app = test_app(&dir.path().join("opora.db"));

        app.sleep_text = "много".to_string();
        app.save();

        assert!(app.status.is_none());
        assert!(app.error.is_some());
    }
}
