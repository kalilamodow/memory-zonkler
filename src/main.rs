use crate::scanner::{FoundAddress, ScanResult, Scanner};
use iced::{Element, widget};
use sysinfo::System;

mod scanner;

const PROCESS_NAME: &str = "test.exe";

#[derive(Clone)]
enum Message {
    OpenProcess,
    SetNumberToCheck(String),
    CheckNumber,
    UpdateFoundValue,
    Reset,
}

#[derive(Default)]
enum AppState {
    #[default]
    WaitingForTarget,
    TargetOpened {
        scanner: Scanner,
        to_check: String,
        possibilities: Option<usize>,
    },
    GotAddress {
        addr: FoundAddress,
        current_value: i32,
    },
    Failed(String),
}

impl AppState {
    fn update(&mut self, message: Message) {
        match (&mut *self, message) {
            (Self::Failed(_), Message::Reset) => {
                *self = Self::WaitingForTarget;
            }
            (Self::WaitingForTarget, Message::OpenProcess) => {
                let pid = {
                    let info = System::new_all();
                    let mut pid: Option<u32> = None;
                    for process in info.processes_by_name(PROCESS_NAME.as_ref()) {
                        pid = Some(process.pid().as_u32());
                    }
                    pid
                };

                let Some(pid) = pid else {
                    *self = Self::Failed("Could not locate process".into());
                    return;
                };

                *self = Self::TargetOpened {
                    scanner: Scanner::new(pid),
                    to_check: "".into(),
                    possibilities: None,
                };
            }
            (
                Self::TargetOpened {
                    scanner: _,
                    to_check,
                    possibilities: _,
                },
                Message::SetNumberToCheck(n),
            ) => {
                *to_check = n;
            }
            (
                Self::TargetOpened {
                    scanner,
                    to_check,
                    possibilities,
                },
                Message::CheckNumber,
            ) => {
                let Ok(number) = to_check.parse::<i32>() else {
                    return;
                };

                match scanner.check(number) {
                    ScanResult::Failed => {
                        *self = Self::Failed("Ran out of possibilities".into());
                    }
                    ScanResult::NotFoundYet(new_possibilities) => {
                        possibilities.replace(new_possibilities);
                    }
                    ScanResult::Found(found) => {
                        let cur = found.read();
                        *self = Self::GotAddress {
                            addr: found,
                            current_value: cur,
                        };
                    }
                }
            }
            (
                Self::GotAddress {
                    addr,
                    current_value,
                },
                Message::UpdateFoundValue,
            ) => {
                *current_value = addr.read();
            }
            _ => {}
        }
    }

    fn view(&self) -> Element<'_, Message> {
        widget::center(match self {
            Self::Failed(err) => widget::column![
                widget::text("Failed :("),
                widget::text!("Error: {err}"),
                widget::button("Reset").on_press(Message::Reset)
            ],
            Self::WaitingForTarget => widget::column![
                widget::text("Welcome!"),
                widget::button("Load game memory").on_press(Message::OpenProcess)
            ],
            Self::TargetOpened {
                scanner: _,
                to_check,
                possibilities,
            } => widget::column![
                widget::text("Process loaded :)"),
                widget::text_input("Check against...", &to_check)
                    .on_input(Message::SetNumberToCheck),
                widget::button("Check").on_press(Message::CheckNumber),
                widget::text(format!(
                    "{} possibilities",
                    possibilities
                        .as_ref()
                        .map(usize::to_string)
                        .unwrap_or_else(|| "Infinite".into())
                ))
            ],
            Self::GotAddress {
                addr: _,
                current_value,
            } => widget::column![
                widget::text("Found!"),
                widget::text(format!("Current value: {current_value}")),
                widget::button("Update").on_press(Message::UpdateFoundValue)
            ],
        })
        .into()
    }
}

fn main() -> iced::Result {
    iced::application(AppState::default, AppState::update, AppState::view)
        .title("infinite iron!!!")
        .window_size((300, 200))
        .run()
}
