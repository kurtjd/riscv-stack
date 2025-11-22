# riscv-stack
Methods for RISCV processors to determine stack size and how much of the stack has been or is being used.

This is a fork of [cortex-m-stack](https://github.com/OpenDevicePartnership/cortex-m-stack) but ported
to RISCV.

**Warning**: this crate depends on the `_stack_start` and `_hart_stack_size` symbols being set correctly.
Correctly means `_hart_stack_size` is equal among all harts therefore the beginning of each hart's stack
can be found at `_stack_start - (_hart_stack_size * hart_id)` (with adjustments for alignment).
The linker script provided by `riscv-rt` should satisfy these requirements.

## Immediate stack usage
Use [current_stack_in_use] or [current_stack_free] to keep track of the memory usage at run-time.

## Historical stack usage
First paint the stack using [repaint_stack] and then measure using [stack_painted] or [stack_painted_binary] to figure out how much stack was used between these two points.
