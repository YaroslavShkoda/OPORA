use chrono::{Local, NaiveDate};
use eframe::egui;
use opora::db;
use opora::entry::{self, Entry};
use rusqlite::Connection;

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
    pub fn new(conn: Connection) -> Self {
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
            }
            Ok(None) => {
                self.sleep_unknown = false;
                self.sleep_text.clear();
                self.mood = 5;
                self.note.clear();
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
                self.status = Some(format!("Сохранено в {}", Local::now().format("%H:%M")));
            }
            Err(err) => self.error = Some(err),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.heading("OPORA");
            ui.label(self.date.format("%d.%m.%Y").to_string());
            ui.add_space(12.0);

            egui::Grid::new("form")
                .num_columns(2)
                .spacing([16.0, 10.0])
                .show(ui, |ui| {
                    ui.label("Сон, часов");
                    ui.horizontal(|ui| {
                        ui.add_enabled(
                            !self.sleep_unknown,
                            egui::TextEdit::singleline(&mut self.sleep_text)
                                .desired_width(70.0)
                                .hint_text("7,5"),
                        );
                        ui.checkbox(&mut self.sleep_unknown, "не помню");
                    });
                    ui.end_row();

                    ui.label("Настроение");
                    ui.add(egui::Slider::new(&mut self.mood, 1..=10));
                    ui.end_row();

                    ui.label("Что было");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.note)
                            .hint_text("одна строка")
                            .desired_width(f32::INFINITY),
                    );
                    ui.end_row();
                });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui
                    .add_sized([140.0, 28.0], egui::Button::new("Сохранить"))
                    .clicked()
                {
                    self.save();
                }
                if let Some(status) = &self.status {
                    ui.weak(status);
                }
            });

            if let Some(err) = &self.error {
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(222, 92, 92), err);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_writes_day_to_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("opora.db");
        let mut app = App::new(db::open(&path).unwrap());

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
        let mut app = App::new(db::open(&dir.path().join("opora.db")).unwrap());

        app.sleep_text = "много".to_string();
        app.save();

        assert!(app.status.is_none());
        assert!(app.error.is_some());
    }
}
