#![no_std]
#![doc = include_str!(concat!("../", env!("CARGO_PKG_README")))]

use core::{arch::asm, mem::size_of, ops::Range};

/// The value used to paint the stack.
pub const STACK_PAINT_VALUE: u32 = 0xCCCC_CCCC;

/// The [Range] currently in use for the current hart's stack.
///
/// Note: the stack is defined in reverse, as it runs from 'start' to 'end' downwards.
/// Hence this range is technically empty because `start >= end`.
///
/// *Important*: `end` represents one past the last value in the stack belonging to current hart,
/// so do not attempt to write to it, as you'd overwrite the start of stack of another hart.
///
/// If you want to use this range to do range-like things, use [stack_rev] instead.
#[inline]
pub fn stack() -> Range<*mut u32> {
    unsafe extern "C" {
        static mut _stack_start: u32;
        static _hart_stack_size: usize;
    }

    // Current hart's ID
    let hartid: usize;
    // SAFETY: We are just reading from a CSR
    unsafe { asm!("csrr {}, mhartid", out(reg) hartid) };

    // The _hart_stack_size symbol's value, which is the size obviously,
    // is represented by the address of the symbol.
    //
    // So we have to first make a fake pointer then treat it as an actual usize.
    let stksz = &raw const _hart_stack_size as usize;

    // Each hart has equal (_hart_stack_size) stack sizes.
    //
    // Thus the Nth hart's stack can be found by offsetting from the very top of stack
    // down to _hart_stack_size * hartid.
    //
    // The below safety requirements would only end up violated if linker script is incorrect.
    // The linker script from `riscv-rt` should satisfy these requirements.
    //
    // SAFETY: Linker script must ensure that `_stack_start - (_hart_stack_size * hartid)`
    // is always within the available stack space.
    let start = unsafe { (&raw mut _stack_start).byte_sub(hartid * stksz) };
    // SAFETY: Linker script must also ensure that the above address, offset downward by another
    // _hart_stack_size, is always within the available stack space and does not interfere
    // with another hart's stack.
    let end = unsafe { start.byte_sub(stksz) };

    // But we want to ensure boundaries are 4 byte aligned before dereferencing them.
    //
    // So different harts will actually have slightly different stack sizes depending
    // on if _hart_stack_size is divisble by 4 or not.
    let start = start.map_addr(|p| p & !0b11);
    let end = end.map_addr(|p| p & !0b11);

    start..end
}

/// The [Range] currently in use for the current hart's stack,
/// defined in reverse such that [Range] operations are viable.
///
/// Hence the `end` of this [Range] is one past where the current hart's stack starts,
/// so it is important not to write to it, otherwise the end of another hart's stack may be overwritten.
#[inline]
pub fn stack_rev() -> Range<*mut u32> {
    // SAFETY: Range states start <= x < end, so when reversing, we add 1 to each bound
    // to keep meaning
    unsafe { stack().end.add(1)..stack().start.add(1) }
}

/// Convenience function to fetch the current hart's stack pointer.
#[inline]
pub fn current_stack_ptr() -> *mut u32 {
    let res;
    // SAFETY: Just reading the stack pointer nothing crazy
    unsafe { asm!("mv {}, sp", out(reg) res) };
    res
}

/// The number of bytes that are reserved for the current hart's stack at compile time.
///
/// Note: Although all harts have equal stack space reserved, their effective stack space
/// may differ slightly due to alignment issues.
#[inline]
pub fn stack_size() -> usize {
    // SAFETY: start >= end. If this is not the case your linker did something wrong.
    unsafe { stack().start.byte_offset_from_unsigned(stack().end) }
}

/// The number of bytes of the current hart's stack that are currently in use.
#[inline]
pub fn current_stack_in_use() -> usize {
    // SAFETY: start >= end. If this is not the case your linker did something wrong.
    unsafe { stack().start.byte_offset_from_unsigned(current_stack_ptr()) }
}

/// The number of bytes of the current hart's stack that are currently free.
///
/// If the stack has overflowed, this function returns 0.
#[inline]
pub fn current_stack_free() -> usize {
    stack_size().saturating_sub(current_stack_in_use())
}

/// What fraction of the current hart's stack is currently in use.
#[inline]
pub fn current_stack_fraction() -> f32 {
    current_stack_in_use() as f32 / stack_size() as f32
}

/// Paint the part of the current hart's stack that is currently not in use.
///
/// **Note:** this can take some time, and an ISR could possibly interrupt this process,
/// dirtying up your freshly painted stack.
/// If you wish to prevent this, run this inside a critical section using `riscv::interrupt::free`.
///
/// Runs in *O(n)* where *n* is the size of the stack.
/// This function is inefficient in the sense that it repaints the entire stack,
/// even the parts that still have the [STACK_PAINT_VALUE].
#[inline(never)]
pub fn repaint_stack() {
    // SAFETY: `stack()` has ensured we are staying within the bounds of the current hart's stack
    unsafe {
        asm!(
            "0:",
            "bgeu {ptr}, sp, 1f",
            "sw {paint}, 0({ptr})",
            "addi {ptr}, {ptr}, 4",
            "j 0b",
            "1:",
            ptr = inout(reg) stack().end.add(1) => _,
            paint = in(reg) STACK_PAINT_VALUE,
        )
    };
}

/// Finds the number of bytes that have not been overwritten on the current hart's stack since the last repaint.
///
/// In other words: shows the worst case free stack space since [repaint_stack] was last called.
///
/// This measurement can only ever be an ESTIMATE, and not a guarantee, as the amount of
/// stack can change immediately, even during an interrupt while we are measuring, or
/// by a devious user or compiler that re-paints the stack, obscuring the max
/// measured value. This measurement MUST NOT be used for load-bearing-safety
/// guarantees, only as a (generally accurate but non-guaranteed) measurement.
///
/// Runs in *O(n)* where *n* is the size of the stack.
#[inline(never)]
pub fn stack_painted() -> usize {
    let res: *const u32;
    // SAFETY: As per the [rust reference], inline asm is allowed to look below the
    // stack pointer. We read the values between the end of stack and the current stack
    // pointer, which are all valid locations.
    //
    // In the case of interruption, there could be false negatives where we don't see
    // stack that was used "behind" our cursor, however this is fine because we do not
    // rely on this number for any safety-bearing contents, only as a metrics estimate.
    //
    // [rust reference]: https://doc.rust-lang.org/reference/inline-assembly.html#r-asm.rules.stack-below-sp
    unsafe {
        asm!(
            "0:",
            "bgeu {ptr}, sp, 1f",
            "lw {value}, 0({ptr})",
            "bne {value}, {paint}, 1f",
            "addi {ptr}, {ptr}, 4",
            "j 0b",
            "1:",
            ptr = inout(reg) stack().end.add(1) => res,
            value = out(reg) _,
            paint = in(reg) STACK_PAINT_VALUE,
            options(nostack, readonly)
        )
    };
    // SAFETY: res >= stack.end() because we start at stack.end()
    unsafe { res.byte_offset_from_unsigned(stack().end) }
}

/// Finds the number of bytes that have not been overwritten on the current hart's stack since the last repaint using binary search.
///
/// In other words: shows the worst case free stack space since [repaint_stack] was last called.
///
/// Uses binary search to find the point after which the stack is written.
/// This will assume that the stack is written in a consecutive fashion.
/// Writing somewhere out-of-order into the painted stack will not be detected.
///
/// Runs in *O(log(n))* where *n* is the size of the stack.
///
/// **Danger:** if the current (active) stack contains the [STACK_PAINT_VALUE] this computation may be very incorrect.
///
/// # Safety
///
/// This function aliases the inactive stack, which is considered to be Undefined Behaviour.
/// Do not use if you care about such things.
pub unsafe fn stack_painted_binary() -> usize {
    // SAFETY: we should be able to read anywhere on the stack using this,
    // but this is considered UB because we are aliasing memory out of nowhere.
    // Will probably still work though.
    let slice =
        unsafe { &*core::ptr::slice_from_raw_parts(stack().end.add(1), current_stack_free() / 4) };
    slice.partition_point(|&word| word == STACK_PAINT_VALUE) * size_of::<usize>()
}
