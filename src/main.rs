use crate::scanner::{FoundAddress, ScanResult, Scanner};
use eframe::egui::{Context, Direction, Ui};
use eframe::{Frame, egui};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread;
use sysinfo::{ProcessesToUpdate, System};

mod scanner;

struct FoundAddressState {
    memory: FoundAddress,
    current_value: i32,
    new: String,
}

impl FoundAddressState {
    fn update(&mut self) {
        self.current_value = self.memory.read();
    }

    fn try_write(&mut self) -> Option<()> {
        let value = self.new.parse::<i32>().ok()?;
        self.memory.write(value);
        self.update();
        Some(())
    }
}

impl From<FoundAddress> for FoundAddressState {
    fn from(value: FoundAddress) -> Self {
        Self {
            current_value: value.read(),
            memory: value,
            new: "".into(),
        }
    }
}

struct ChoosableProcess {
    name: String,
    pid: u32,
}

enum AppState {
    WaitingForTarget {
        processes: Vec<ChoosableProcess>,
        filter_input: String,
    },
    TargetOpened {
        scanner: Scanner,
        to_check: String,
        possibilities: Option<usize>,
    },
    Scanning(AtomicU8), // 0-100
    FoundAddresses(Vec<FoundAddressState>),
    Failed(String),
}

impl Default for AppState {
    fn default() -> Self {
        let mut info = System::new();
        info.refresh_processes(ProcessesToUpdate::All, true);
        Self::WaitingForTarget {
            processes: info
                .processes()
                .into_iter()
                .map(|(pid, process)| ChoosableProcess {
                    pid: pid.as_u32(),
                    name: process.name().to_string_lossy().into_owned(),
                })
                .collect(),
            filter_input: "".into(),
        }
    }
}

impl AppState {
    fn reset(&mut self) {
        *self = Self::default();
    }

    fn open_target(&mut self, pid: u32) {
        *self = Self::TargetOpened {
            scanner: Scanner::new(pid),
            to_check: "".into(),
            possibilities: None,
        }
    }

    fn done_scanning(&mut self, scanner: Scanner, scan_result: ScanResult) {
        match scan_result {
            ScanResult::Found(possibilities) => {
                *self = Self::TargetOpened {
                    scanner,
                    to_check: "".into(),
                    possibilities: Some(possibilities),
                }
            }
            ScanResult::Failed => {
                self.failed("Scan failed".into());
            }
        }
    }

    fn accept(&mut self) {
        let old_self = std::mem::replace(
            self,
            Self::Failed(
                "invalid state (AppState::accept without being AppState::TargetOpened)".into(),
            ),
        );
        let Self::TargetOpened { scanner, .. } = old_self else {
            return;
        };
        *self = Self::FoundAddresses(scanner.accept().into_iter().map(Into::into).collect());
    }

    fn failed(&mut self, error: String) {
        *self = Self::Failed(error);
    }
}

fn short_input(text: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(text).desired_width(100.0)
}

enum ScanProgressMessage {
    Progressed(u8),
    Done(Scanner, ScanResult),
}

#[derive(Default)]
struct App {
    state: AppState,
    scan_progress_rx: Option<mpsc::Receiver<ScanProgressMessage>>,
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &Context, _: &mut Frame) {
        if let Some(rx) = &self.scan_progress_rx {
            if let AppState::Scanning(atom) = &mut self.state {
                ctx.request_repaint();
                for msg in rx.try_iter() {
                    match msg {
                        ScanProgressMessage::Progressed(p) => {
                            atom.store(p, Ordering::Relaxed);
                        }
                        ScanProgressMessage::Done(scanner, result) => {
                            self.state.done_scanning(scanner, result);
                            break;
                        }
                    }
                }
            } else {
                self.scan_progress_rx = None;
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut Frame) {
        egui::CentralPanel::default_margins().show(ui, |ui| match &mut self.state {
            AppState::WaitingForTarget {
                processes,
                filter_input,
            } => {
                ui.horizontal(|ui| {
                    ui.label("Choose a process");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                        ui.add(short_input(filter_input).hint_text("Filter..."));
                    });
                });

                ui.separator();

                let mut pid_to_open: Option<u32> = None;
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        for choice in processes.iter().filter(|p| {
                            p.name.to_lowercase().contains(&filter_input.to_lowercase())
                        }) {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(&choice.name);
                                    ui.small(choice.pid.to_string());

                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Min),
                                        |ui| {
                                            if ui.button("Choose").clicked() {
                                                pid_to_open = Some(choice.pid);
                                            }
                                        },
                                    );
                                });
                            });
                        }
                    });
                });

                if let Some(pid) = pid_to_open {
                    self.state.open_target(pid);
                }
            }
            AppState::TargetOpened {
                scanner: _,
                to_check,
                possibilities,
            } => {
                ui.label("Target process opened!");

                let mut should_run_filter = false;
                ui.horizontal(|ui| {
                    ui.label("Next value:");
                    ui.add(short_input(to_check));
                    should_run_filter = ui.button("Filter").clicked();
                });

                ui.add_space(4.0);

                let mut should_accept = false;
                ui.horizontal(|ui| {
                    ui.label(format!(
                        "Current # of possibilities: {}",
                        possibilities
                            .as_ref()
                            .map(usize::to_string)
                            .unwrap_or_else(|| "Infinite".into())
                    ));
                    should_accept = ui.button("Accept").clicked();
                });

                if should_run_filter && let Ok(check) = to_check.parse::<i32>() {
                    let progress = AtomicU8::new(0);
                    let (tx, rx) = mpsc::channel();
                    self.scan_progress_rx = Some(rx);
                    let state = std::mem::replace(&mut self.state, AppState::Scanning(progress));
                    let mut scanner = match state {
                        AppState::TargetOpened { scanner, .. } => scanner,
                        _ => unreachable!("enum switched variants randomly"),
                    };

                    thread::spawn(move || {
                        let result = scanner.scan(check, |p| {
                            let _ =
                                tx.send(ScanProgressMessage::Progressed((p * 100.0).round() as u8));
                        });
                        tx.send(ScanProgressMessage::Done(scanner, result)).unwrap();
                    });
                } else if should_accept {
                    self.state.accept();
                }
            }
            AppState::Scanning(progress) => {
                let progress = progress.load(Ordering::Relaxed) as f32 / 100.0;
                ui.strong("Scanning...");
                ui.add(egui::ProgressBar::new(progress));
                ui.request_repaint();
            }
            AppState::FoundAddresses(addrs) => {
                let mut should_reset = false;
                ui.horizontal(|ui| {
                    ui.strong(format!("Found {} addresses", addrs.len()));
                    if ui.button("Done").clicked() {
                        should_reset = true;
                    }
                });
                if should_reset {
                    self.state.reset();
                    return;
                }

                ui.add_space(8.0);
                ui.vertical(|ui| {
                    for address in &mut *addrs {
                        ui.group(|ui| {
                            ui.columns_const(|[c1, c2]| {
                                c1.small(address.memory.format());
                                c1.add_space(4.0);
                                c1.horizontal(|ui| {
                                    ui.add(short_input(&mut address.new));
                                    if ui.button("Write").clicked() {
                                        address.try_write();
                                    }
                                });

                                c2.with_layout(
                                    egui::Layout::from_main_dir_and_cross_align(
                                        Direction::TopDown,
                                        egui::Align::Max,
                                    ),
                                    |ui| {
                                        ui.label(format!("Current: {}", address.current_value));
                                        if ui.button("Update").clicked() {
                                            address.update();
                                        }
                                    },
                                );
                            });
                        });
                    }
                });

                ui.add_space(4.0);
                if ui.button("Update all").clicked() {
                    for addr in addrs {
                        addr.update();
                    }
                }
            }
            AppState::Failed(error) => {
                ui.strong("Failure:");
                ui.colored_label(ui.visuals().error_fg_color, error);
                if ui.button("Reset").clicked() {
                    self.state.reset();
                }
            }
        });
    }
}

fn main() -> eframe::Result {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([400.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "memory zonkler",
        native_options,
        Box::new(|_cc| Ok(Box::<App>::default())),
    )
}
