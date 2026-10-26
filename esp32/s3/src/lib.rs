#![no_std]
// Inline assembly on Xtensa (reading `epc1` in the profiler) is still unstable.
#![feature(asm_experimental_arch)]

pub mod profiler;
