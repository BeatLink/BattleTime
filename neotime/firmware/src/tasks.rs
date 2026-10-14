//! The task table the system app's task page lists, kept from embassy's trace hooks.
//!
//! The busy time counts in the clock's 32,768 ticks a second and wraps after about 36 hours of running.
//!
//! Every task names itself with `name_this_task` when it starts, since the hooks know it only by its address.

use core::cell::UnsafeCell;

use embassy_time::{Instant, TICK_HZ};
use neotime_task_table::{ticks_to_milliseconds, Task, TaskTable};

/// The table, which only thread mode touches: the executor's hooks and the tasks it polls, never an interrupt.
struct ThreadModeTable(UnsafeCell<TaskTable>);

// Safety: the firmware runs one thread-mode executor and no interrupt handler reaches the table, so no two accesses overlap.
unsafe impl Sync for ThreadModeTable {}

static TABLE: ThreadModeTable = ThreadModeTable(UnsafeCell::new(TaskTable::new()));

/// Runs `work` on the table; each caller finishes before the next can start, as all run in thread mode.
fn with_table<Answer>(work: impl FnOnce(&mut TaskTable) -> Answer) -> Answer {
    work(unsafe { &mut *TABLE.0.get() })
}

/// Names the task that calls it; call it first thing in a task.
pub fn name_this_task(name: &'static str) {
    with_table(|table| table.name_running(name));
}

#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn count() -> usize {
    with_table(|table| table.len())
}

/// One task, by its place in the table, or none past its end.
#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn task(place: usize) -> Option<Task> {
    with_table(|table| table.get(place).copied())
}

#[cfg_attr(not(with_shell), allow(dead_code))]
pub fn busy_milliseconds(task: &Task) -> u64 {
    ticks_to_milliseconds(task.busy_ticks, TICK_HZ)
}

// ============================================================================
// embassy's trace hooks
// ============================================================================

#[no_mangle]
fn _embassy_trace_task_new(_executor_id: u32, task_id: u32) {
    with_table(|table| table.add(task_id));
}

#[no_mangle]
fn _embassy_trace_task_end(_executor_id: u32, task_id: u32) {
    with_table(|table| table.remove(task_id));
}

#[no_mangle]
fn _embassy_trace_task_exec_begin(_executor_id: u32, task_id: u32) {
    let now = Instant::now().as_ticks() as u32;
    with_table(|table| table.start_running(task_id, now));
}

#[no_mangle]
fn _embassy_trace_task_exec_end(_executor_id: u32, _task_id: u32) {
    let now = Instant::now().as_ticks() as u32;
    with_table(|table| table.stop_running(now));
}

#[no_mangle]
fn _embassy_trace_poll_start(_executor_id: u32) {}

#[no_mangle]
fn _embassy_trace_task_ready_begin(_executor_id: u32, _task_id: u32) {}

#[no_mangle]
fn _embassy_trace_executor_idle(_executor_id: u32) {}
