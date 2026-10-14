//! The firmware's tasks, with how often each has run and how long it has spent running.
//!
//! The firmware feeds it from embassy's trace hooks: a task is added when it is spawned, and timed
//! each time the executor polls it. A task names itself on its first run, since the hooks know a
//! task only by its address. A task that ends leaves the table.

#![no_std]

#[cfg(test)]
mod tests;

// ============================================================================
// The table
// ============================================================================

/// Room for every task the firmware spawns, `main` among them; a task past this is not listed.
pub const MOST_TASKS: usize = 12;

/// Shown for a task that has not named itself.
pub const UNNAMED: &str = "?";

/// Every field starts at zero, so the table needs no flash to start from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task {
    /// The task's address, as the trace hooks give it.
    id: u32,
    name: Option<&'static str>,
    /// How many times the executor has polled it.
    pub runs: u32,
    /// How long those polls took, in the clock's ticks, counting round past the largest `u32`.
    pub busy_ticks: u32,
}

impl Task {
    pub fn name(&self) -> &'static str {
        self.name.unwrap_or(UNNAMED)
    }
}

pub struct TaskTable {
    tasks: [Task; MOST_TASKS],
    count: usize,
    /// One more than the place of the task being polled now, or 0 between polls.
    running_place_plus_one: usize,
    running_since_tick: u32,
}

impl Default for TaskTable {
    fn default() -> Self {
        TaskTable::new()
    }
}

impl TaskTable {
    pub const fn new() -> TaskTable {
        TaskTable {
            tasks: [Task {
                id: 0,
                name: None,
                runs: 0,
                busy_ticks: 0,
            }; MOST_TASKS],
            count: 0,
            running_place_plus_one: 0,
            running_since_tick: 0,
        }
    }

    /// A task was spawned.
    pub fn add(&mut self, id: u32) {
        if self.count < MOST_TASKS {
            self.tasks[self.count] = Task {
                id,
                name: None,
                runs: 0,
                busy_ticks: 0,
            };
            self.count += 1;
        }
    }

    /// A task finished, so it leaves the table and the ones after it move up.
    pub fn remove(&mut self, id: u32) {
        if let Some(place) = self.place_of(id) {
            self.tasks.copy_within(place + 1..self.count, place);
            self.count -= 1;
            self.running_place_plus_one = 0;
        }
    }

    /// The executor began polling a task.
    pub fn start_running(&mut self, id: u32, now_tick: u32) {
        self.running_place_plus_one = self.place_of(id).map_or(0, |place| place + 1);
        self.running_since_tick = now_tick;
    }

    /// The executor finished polling a task.
    pub fn stop_running(&mut self, now_tick: u32) {
        let ran_for = now_tick.wrapping_sub(self.running_since_tick);
        if let Some(task) = self.running_task() {
            task.runs = task.runs.wrapping_add(1);
            task.busy_ticks = task.busy_ticks.wrapping_add(ran_for);
        }
        self.running_place_plus_one = 0;
    }

    /// Names the task being polled now; a task calls this when it starts.
    pub fn name_running(&mut self, name: &'static str) {
        if let Some(task) = self.running_task() {
            task.name = Some(name);
        }
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn get(&self, place: usize) -> Option<&Task> {
        self.tasks[..self.count].get(place)
    }

    fn running_task(&mut self) -> Option<&mut Task> {
        let place = self.running_place_plus_one.checked_sub(1)?;
        self.tasks.get_mut(place)
    }

    fn place_of(&self, id: u32) -> Option<usize> {
        self.tasks[..self.count].iter().position(|task| task.id == id)
    }
}

/// Ticks of a clock running at `ticks_per_second`, in whole milliseconds.
pub fn ticks_to_milliseconds(ticks: u32, ticks_per_second: u64) -> u64 {
    u64::from(ticks) * 1000 / ticks_per_second
}
