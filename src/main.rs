mod scanner;

use crate::scanner::{ScanResult, Scanner};
use std::io;
use sysinfo::System;

fn wait_for_enter_key() {
    let mut buf = String::with_capacity(2);
    let _ = io::stdin().read_line(&mut buf);
}

const PROCESS_NAME: &str = "test.exe";

fn main() {
    let pid = {
        let info = System::new_all();
        let mut pid: Option<u32> = None;
        for process in info.processes_by_name(PROCESS_NAME.as_ref()) {
            pid = Some(process.pid().as_u32());
        }
        pid.expect("could not find target process")
    };

    let mut checks = [50, 49, 52, 48, 40, 60].iter();
    let mut scanner = Scanner::new(pid);

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
