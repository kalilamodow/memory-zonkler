use byteorder::{LittleEndian, ReadBytesExt};
use std::ffi::c_void;
use std::io;
use std::io::{Cursor, Seek};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualQueryEx,
};
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

fn process_handle() -> HANDLE {
    let process = unsafe {
        windows::Win32::System::Threading::OpenProcess(
            PROCESS_VM_READ | PROCESS_QUERY_INFORMATION,
            false,
            16516,
        )
        .expect("could not open process")
    };

    process
}

fn get_system_info() -> SYSTEM_INFO {
    let mut info = SYSTEM_INFO::default();
    unsafe {
        GetSystemInfo(&mut info);
    }
    info
}

fn query_mem_info(process_handle: HANDLE, address: *const c_void) -> MEMORY_BASIC_INFORMATION {
    let mut info = MEMORY_BASIC_INFORMATION::default();
    unsafe {
        VirtualQueryEx(
            process_handle,
            Some(address),
            &mut info,
            size_of::<MEMORY_BASIC_INFORMATION>(),
        );
    }

    info
}

fn region_is_writable(protect: u32) -> bool {
    protect & (PAGE_READWRITE.0 | PAGE_EXECUTE_READWRITE.0) != 0
}

fn read_process_memory(
    process_handle: HANDLE,
    address: usize,
    size: usize,
) -> windows::core::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(size);
    let mut bytes_read = 0usize;

    let read_result = unsafe {
        ReadProcessMemory(
            process_handle,
            address as *const c_void,
            buf.as_mut_ptr() as *mut c_void,
            size,
            Some(&mut bytes_read),
        )
    };

    println!("tried reading {size} bytes @ {address}. read: {bytes_read} bytes");
    if read_result.is_err() {
        eprintln!("failure!");
        read_result?;
    }

    unsafe {
        buf.set_len(size);
    }

    Ok(buf)
}

fn read_regions(process_handle: HANDLE) -> Vec<MemoryRegion> {
    let system_info = get_system_info();

    let mut current_address = system_info.lpMinimumApplicationAddress as usize;
    let maximum_address = system_info.lpMaximumApplicationAddress as usize;

    let mut regions: Vec<MemoryRegion> = vec![];

    while current_address < maximum_address {
        let mem_info = query_mem_info(process_handle, current_address as *const c_void);
        let base = mem_info.BaseAddress as usize;
        let region_end = base + mem_info.RegionSize;

        if mem_info.State != MEM_COMMIT || !region_is_writable(mem_info.Protect.0) {
            current_address = region_end;
            continue;
        }

        if let Ok(memory) =
            read_process_memory(process_handle, current_address, mem_info.RegionSize)
        {
            regions.push(MemoryRegion {
                base_address: current_address,
                contents: memory,
            });
        }

        current_address = region_end;
    }

    regions
}

type Address = usize;

fn scan_regions(regions: &[MemoryRegion], check: i32) -> Vec<Address> {
    let mut result = vec![];

    for region in regions {
        let mut cursor = Cursor::new(&region.contents);
        while let Ok(number) = cursor.read_i32::<LittleEndian>() {
            if number == check {
                result.push(region.base_address + cursor.position() as usize);
            }

            // because read_i32 moves forward by 4 bytes
            cursor.seek_relative(-3).unwrap();
        }
    }

    result
}

struct MemoryRegion {
    base_address: Address,
    contents: Vec<u8>,
}

fn main() {
    let handle = process_handle();

    let checks = [50, 49, 52, 48, 40, 60];
    let mut possible_addresses: Option<Vec<Address>> = None;

    for check in checks {
        println!(
            "current possible addresses len: {:?}",
            possible_addresses.as_ref().map(Vec::len)
        );
        println!("set counter to {check}, then press enter");
        {
            let mut buf = String::with_capacity(2);
            let _ = io::stdin().read_line(&mut buf);
        }

        let regions = read_regions(handle);
        let found = scan_regions(&regions, check);

        if let Some(possible_addresses) = &mut possible_addresses {
            possible_addresses.retain(|addr| found.contains(addr));
        } else {
            possible_addresses = Some(found);
        }

        match possible_addresses.as_ref().map(Vec::len) {
            Some(1) => break,
            Some(0) => {
                eprintln!("failed to locate address");
                return;
            }
            _ => {}
        }
    }

    let address = possible_addresses.unwrap()[0];
    println!("address: {address}");
}
