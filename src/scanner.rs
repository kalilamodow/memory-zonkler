use byteorder::{LittleEndian, ReadBytesExt};
use rayon::prelude::*;
use std::collections::HashMap;
use std::ffi::c_void;
use std::io::Cursor;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Diagnostics::Debug::{ReadProcessMemory, WriteProcessMemory};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READWRITE, PAGE_READWRITE, VirtualQueryEx,
};
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::System::Threading::{
    PROCESS_QUERY_INFORMATION, PROCESS_VM_READ, PROCESS_VM_WRITE,
};

#[derive(Hash, Eq, PartialEq)]
pub struct MemoryRegion {
    base_address: BaseAddress,
    contents: Vec<u8>,
}

pub type Address = usize;
type BaseAddress = Address;

#[derive(Clone, Copy)]
struct ProcessHandle(usize);

impl From<HANDLE> for ProcessHandle {
    fn from(value: HANDLE) -> Self {
        ProcessHandle(value.0 as usize)
    }
}

impl From<ProcessHandle> for HANDLE {
    fn from(value: ProcessHandle) -> Self {
        HANDLE(value.0 as *mut c_void)
    }
}

fn get_system_info() -> SYSTEM_INFO {
    let mut info = SYSTEM_INFO::default();
    unsafe {
        GetSystemInfo(&mut info);
    }
    info
}

fn query_mem_info(
    process_handle: ProcessHandle,
    address: *const c_void,
) -> MEMORY_BASIC_INFORMATION {
    let mut info = MEMORY_BASIC_INFORMATION::default();
    unsafe {
        VirtualQueryEx(
            process_handle.into(),
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
    process_handle: ProcessHandle,
    address: BaseAddress,
    size: usize,
) -> windows::core::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(size);
    let mut bytes_read = 0usize;

    let read_result = unsafe {
        ReadProcessMemory(
            process_handle.into(),
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

fn query_memory_regions(process_handle: ProcessHandle) -> Vec<MemoryRegion> {
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

        if let Ok(memory) = read_process_memory(process_handle, base, mem_info.RegionSize) {
            regions.push(MemoryRegion {
                base_address: current_address,
                contents: memory,
            });
        }

        current_address = region_end;
    }

    regions
}

fn initial_scan_regions<OnProgressFn: Fn(f32) + Send + Sync>(
    regions: Vec<MemoryRegion>,
    check: i32,
    on_progress: OnProgressFn,
) -> HashMap<MemoryRegion, Vec<Address>> {
    let progress = AtomicUsize::new(0);
    let total_regions = regions.len() as f32;
    regions
        .into_par_iter()
        .map(|region| {
            let result = region
                .contents
                .par_array_windows::<4>()
                .map(|bytes| i32::from_le_bytes(*bytes))
                .enumerate()
                .filter_map(|(offset, v)| (check == v).then_some(offset))
                .map(|offset| region.base_address + offset)
                .collect();

            let prev_progress = progress.fetch_add(1, Ordering::Relaxed);
            println!("progress: {}/{}", prev_progress + 1, total_regions as usize);
            on_progress(prev_progress as f32 / total_regions);

            (region, result)
        })
        .collect()
}

fn pid_to_handle(pid: u32) -> ProcessHandle {
    let process = unsafe {
        windows::Win32::System::Threading::OpenProcess(
            PROCESS_VM_READ | PROCESS_VM_WRITE | PROCESS_QUERY_INFORMATION,
            false,
            pid,
        )
        .expect("could not open process")
    };

    process.into()
}

pub struct FoundAddress {
    address: Address,
    process: ProcessHandle,
}

impl FoundAddress {
    fn new(address: Address, process: ProcessHandle) -> Self {
        FoundAddress { address, process }
    }

    pub fn format(&self) -> String {
        format!("{:x}", self.address)
    }

    pub fn read(&self) -> i32 {
        let mut value = 0i32;

        unsafe {
            ReadProcessMemory(
                self.process.into(),
                self.address as *const c_void,
                &mut value as *mut i32 as *mut c_void,
                size_of::<i32>(),
                None,
            )
            .unwrap();
        }

        value
    }

    pub fn write(&self, val: i32) {
        let buf = val.to_le_bytes();
        unsafe {
            WriteProcessMemory(
                self.process.into(),
                self.address as *const c_void,
                buf.as_ptr() as *const c_void,
                size_of::<i32>(),
                None,
            )
            .unwrap();
        }
    }
}

pub enum ScanResult {
    Found(usize),
    Failed,
}

pub struct Scanner {
    process_handle: ProcessHandle,
    possible: Option<HashMap<MemoryRegion, Vec<Address>>>,
}

impl Scanner {
    pub fn new(pid: u32) -> Self {
        Self {
            process_handle: pid_to_handle(pid),
            possible: None,
        }
    }

    pub fn scan<OnProgressFn: Fn(f32) + Send + Sync>(
        &mut self,
        check: i32,
        on_progress: OnProgressFn,
    ) -> ScanResult {
        let start = Instant::now();
        self.possible = Some(match self.possible.take() {
            None => {
                let regions = query_memory_regions(self.process_handle);
                initial_scan_regions(regions, check, on_progress)
            }
            Some(possible) => {
                let finished = AtomicUsize::new(0);
                let total_regions = possible.len() as f32;

                possible
                    .into_par_iter()
                    .filter_map(|(mut region, mut addrs)| {
                        region.contents = read_process_memory(
                            self.process_handle,
                            region.base_address,
                            region.contents.len(),
                        )
                        .ok()?;

                        let mut cursor = Cursor::new(&region.contents);
                        addrs.retain(|addr| {
                            let relative_offset = addr - region.base_address;
                            cursor.set_position(relative_offset as u64);
                            let value = cursor.read_i32::<LittleEndian>().unwrap();
                            value == check
                        });

                        let new_finished = finished.fetch_add(1, Ordering::Relaxed) + 1;
                        println!("update: finished {new_finished}/{total_regions}");
                        on_progress(new_finished as f32 / total_regions);

                        Some((region, addrs))
                    })
                    .collect()
            }
        });

        eprintln!("scanned in {:?}", start.elapsed());

        let count = self.possible.as_ref().unwrap().values().flatten().count();
        match count {
            0 => ScanResult::Failed,
            n => ScanResult::Found(n),
        }
    }

    pub fn accept(self) -> Vec<FoundAddress> {
        self.possible
            .map(|possible| {
                possible
                    .into_values()
                    .flatten()
                    .map(|addr| FoundAddress::new(addr, self.process_handle))
                    .collect()
            })
            .unwrap_or_default()
    }
}
