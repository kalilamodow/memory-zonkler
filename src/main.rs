use crate::scanner::{FoundAddress, ScanResult, Scanner};
use iced::{Element, widget};
use sysinfo::System;

mod scanner;

#[derive(Clone)]
enum Message {
    SetProcessName(String),
    OpenProcess,
    SetNumberToCheck(String),
    CheckNumber,
    UpdateFoundValue,
    SetWriteInput(String),
    Write,
    Reset,
}

enum AppState {
    WaitingForTarget(String),
    TargetOpened {
        scanner: Scanner,
        to_check: String,
        possibilities: Option<usize>,
    },
    GotAddress {
        addr: FoundAddress,
        current_value: i32,
        input: String,
    },
    Failed(String),
}

impl AppState {
    fn new() -> Self {
        Self::WaitingForTarget("".into())
    }

    fn update(&mut self, message: Message) {
        match (&mut *self, message) {
            (Self::Failed(_), Message::Reset) => {
                *self = Self::WaitingForTarget("".into());
            }
            (Self::WaitingForTarget(process_name), Message::SetProcessName(new_process_name)) => {
                *process_name = new_process_name;
            }
            (Self::WaitingForTarget(process_name), Message::OpenProcess) => {
                let pid = {
                    let info = System::new_all();
                    let mut pid: Option<u32> = None;
                    for process in info.processes_by_name(process_name.as_ref()) {
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
                            input: String::new(),
                        };
                    }
                }
            }
            (
                Self::GotAddress {
                    addr,
                    current_value,
                    input: _,
                },
                Message::UpdateFoundValue,
            ) => {
                *current_value = addr.read();
            }
            (
                Self::GotAddress {
                    addr: _,
                    current_value: _,
                    input,
                },
                Message::SetWriteInput(new_input),
            ) => {
                *input = new_input;
            }

            (
                Self::GotAddress {
                    addr,
                    current_value: _,
                    input,
                },
                Message::Write,
            ) => {
                let Ok(value) = input.parse::<i32>() else {
                    return;
                };

                addr.write(value);
                input.clear();
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
            Self::WaitingForTarget(process_name) => widget::column![
                widget::text("Welcome!"),
                widget::text_input("Process name...", &process_name)
                    .on_input(Message::SetProcessName),
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
                input,
            } => widget::column![
                widget::text("Found!"),
                widget::row![
                    widget::text(format!("Current value: {current_value}")),
                    widget::button("Update").on_press(Message::UpdateFoundValue),
                ],
                widget::row![
                    widget::text_input("Set value...", &input).on_input(Message::SetWriteInput),
                    widget::button("Write value").on_press(Message::Write)
                ],
            ],
        })
        .into()
    }
}

fn main() -> iced::Result {
    iced::application(AppState::new, AppState::update, AppState::view)
        .title("memory zonkler")
        .window_size((300, 200))
        .run()
}
