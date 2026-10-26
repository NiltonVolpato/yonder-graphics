//! Sampling profiler for benchmarks. A timer interrupt records the interrupted program
//! counter; `stop` prints the hot spots, which `tools/symbolize_profile.py` turns into
//! source locations (`just profile` does both).
//!
//! Adapted from the smart-dial profiler. Counts use the space-saving algorithm, so the
//! memory is fixed and the hottest addresses are never dropped.

use core::cell::RefCell;

use critical_section::Mutex;
use esp_hal::peripherals::TIMG1;
use esp_hal::timer::PeriodicTimer;
use esp_hal::timer::timg::TimerGroup;
use esp_hal::{Blocking, handler};
use space_saving::stream_summary::StreamSummary;
use space_saving::summary::{BucketRef, Summary};

/// 2 kHz: frequent enough for a few seconds of benchmark, rare enough not to skew it.
const SAMPLING_PERIOD: esp_hal::time::Duration = esp_hal::time::Duration::from_micros(500);

/// The 1023 hottest addresses, in about 25 KB: enough for every instruction of a hot loop,
/// so the counts are exact unless the profiled code is much larger.
type Hotspots = StreamSummary<1023, 2048, u32, usize, u16>;

struct Samples {
    hotspots: Hotspots,
    total: u32,
}

static TIMER: Mutex<RefCell<Option<PeriodicTimer<'static, Blocking>>>> =
    Mutex::new(RefCell::new(None));
static SAMPLES: Mutex<RefCell<Samples>> = Mutex::new(RefCell::new(Samples {
    hotspots: StreamSummary::new(0, 0),
    total: 0,
}));

/// Starts sampling. Earlier samples are discarded.
pub fn start(timg1: TIMG1<'static>) {
    critical_section::with(|cs| {
        *SAMPLES.borrow_ref_mut(cs) = Samples {
            hotspots: StreamSummary::new(0, 0),
            total: 0,
        };
    });
    let mut timer = PeriodicTimer::new(TimerGroup::new(timg1).timer0);
    timer.set_interrupt_handler(sample);
    timer.listen();
    timer
        .start(SAMPLING_PERIOD)
        .expect("profiler timer period is valid");
    critical_section::with(|cs| TIMER.borrow_ref_mut(cs).replace(timer));
}

/// Stops sampling and prints every sampled address, hottest first.
pub fn stop() {
    critical_section::with(|cs| {
        if let Some(mut timer) = TIMER.borrow_ref_mut(cs).take() {
            timer.unlisten();
        }
    });
    critical_section::with(|cs| {
        let samples = SAMPLES.borrow_ref(cs);
        defmt::info!("[PROFILE] {} samples", samples.total);
        let hottest = samples
            .hotspots
            .iter_buckets()
            .flat_map(BucketRef::iter_items);
        for (rank, item) in hottest.enumerate() {
            let percent = item.count as f32 * 100.0 / samples.total as f32;
            // Space-saving overcounts by at most `item.value`.
            defmt::info!(
                "[PROFILE] #{}: 0x{:08x} - {} samples ({}%) error <= {}",
                rank + 1,
                item.key,
                item.count,
                percent,
                item.value
            );
        }
    });
}

#[handler]
fn sample() {
    // The program counter where the timer interrupted the code being profiled.
    let pc: u32;
    unsafe { core::arch::asm!("rsr.epc1 {0}", out(reg) pc) };
    critical_section::with(|cs| {
        if let Some(timer) = TIMER.borrow_ref_mut(cs).as_mut() {
            timer.clear_interrupt();
        }
        let mut samples = SAMPLES.borrow_ref_mut(cs);
        samples.hotspots.observe(pc);
        samples.total += 1;
    });
}
