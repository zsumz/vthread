use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

static ENABLED: AtomicBool = AtomicBool::new(false);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static DEALLOCATIONS: AtomicU64 = AtomicU64::new(0);
static ALLOCATED_BYTES: AtomicU64 = AtomicU64::new(0);

pub(crate) struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        // SAFETY: the request is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ENABLED.load(Ordering::Relaxed) {
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        // SAFETY: the pointer and layout came from this allocator's system allocation.
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) {
            ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            DEALLOCATIONS.fetch_add(1, Ordering::Relaxed);
            ALLOCATED_BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        // SAFETY: the request is forwarded unchanged to the system allocator.
        unsafe { System.realloc(pointer, layout, size) }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Counts {
    pub(crate) allocations: u64,
    pub(crate) deallocations: u64,
    pub(crate) allocated_bytes: u64,
}

pub(crate) fn begin() {
    ENABLED.store(false, Ordering::SeqCst);
    ALLOCATIONS.store(0, Ordering::Relaxed);
    DEALLOCATIONS.store(0, Ordering::Relaxed);
    ALLOCATED_BYTES.store(0, Ordering::Relaxed);
    ENABLED.store(true, Ordering::SeqCst);
}

pub(crate) fn finish() -> Counts {
    ENABLED.store(false, Ordering::SeqCst);
    Counts {
        allocations: ALLOCATIONS.load(Ordering::Relaxed),
        deallocations: DEALLOCATIONS.load(Ordering::Relaxed),
        allocated_bytes: ALLOCATED_BYTES.load(Ordering::Relaxed),
    }
}

pub(crate) fn print_medians(config: &super::Config, samples: &mut [Counts]) {
    let middle = samples.len() / 2;
    samples.sort_unstable_by_key(|sample| sample.allocations);
    let allocations = samples[middle].allocations;
    samples.sort_unstable_by_key(|sample| sample.deallocations);
    let deallocations = samples[middle].deallocations;
    samples.sort_unstable_by_key(|sample| sample.allocated_bytes);
    let allocated_bytes = samples[middle].allocated_bytes;
    println!(
        "engine={} phase=allocation workers={} tasks={} allocations={} deallocations={} allocated_bytes={} allocations_per_task={:.2} bytes_per_task={:.2}",
        config.engine_name(),
        config.workers,
        config.tasks,
        allocations,
        deallocations,
        allocated_bytes,
        allocations as f64 / config.tasks as f64,
        allocated_bytes as f64 / config.tasks as f64,
    );
}
