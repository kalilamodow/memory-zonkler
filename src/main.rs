mod scanner;

use std::io;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use crate::scanner::{ScanResult, Scanner};

fn process_handle() -> HANDLE {
    let process = unsafe {
        windows::Win32::System::Threading::OpenProcess(
            PROCESS_VM_READ | PROCESS_QUERY_INFORMATION,
            false,
            29196,
        )
            .expect("could not open process")
    };

    process
}
fn wait_for_enter_key() {
    let mut buf = String::with_capacity(2);
    let _ = io::stdin().read_line(&mut buf);
}

fn main() {
    let handle = process_handle();
    let mut checks = [50, 49, 52, 48, 40, 60].iter();
    let mut scanner = Scanner::new(handle);

    let found = loop {
        let check = *checks.next().expect("ran out of checks");

        println!("set counter to {check}, then press enter");
        wait_for_enter_key();

        match scanner.check(check) {
            ScanResult::Found(addr) => break addr,
            ScanResult::NotFoundYet(found) => println!("remaining possibilities: {found}"),
            ScanResult::Failed => {
                eprintln!("failed to find :(");
                return;
            }
        }
    };

    loop {
        let value = found.read();
        println!("value is {value}!");

        println!("press enter to recheck...");
        wait_for_enter_key();
    }
}
