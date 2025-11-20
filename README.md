# riscv-stack
Methods for RISCV processors to determine stack size and how much of the stack has been or is being used.

This is a fork of [cortex-m-stack](https://github.com/OpenDevicePartnership/cortex-m-stack) but ported
to RISCV assembly.

**Warning**: this crate depends on the `_stack_start` and `_stack_end` symbols being set correctly.
Currently, the linker script provided by `riscv-rt` does not export `_stack_end`, but I will open a PR
for my fix soon.

## Immediate stack usage
Use [current_stack_in_use] or [current_stack_free] to keep track of the memory usage at run-time.

## Historical stack usage
First paint the stack using [repaint_stack] and then measure using [stack_painted] or [stack_painted_binary] to figure out how much stack was used between these two points.
