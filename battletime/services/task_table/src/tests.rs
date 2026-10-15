use super::*;

#[test]
fn a_spawned_task_is_listed_unnamed_until_it_names_itself() {
    let mut table = TaskTable::new();
    table.add(0x2000_1000);

    assert_eq!(table.len(), 1);
    assert_eq!(table.get(0).unwrap().name(), UNNAMED);

    table.start_running(0x2000_1000, 10);
    table.name_running("shell");
    table.stop_running(15);

    let task = table.get(0).unwrap();
    assert_eq!(task.name(), "shell");
    assert_eq!(task.runs, 1);
    assert_eq!(task.busy_ticks, 5);
}

#[test]
fn each_poll_adds_a_run_and_its_time_to_its_own_task() {
    let mut table = TaskTable::default();
    table.add(1);
    table.add(2);

    for (id, start, end) in [(1, 0, 3), (2, 3, 4), (1, 10, 20)] {
        table.start_running(id, start);
        table.stop_running(end);
    }

    assert_eq!(
        (table.get(0).unwrap().runs, table.get(0).unwrap().busy_ticks),
        (2, 13)
    );
    assert_eq!(
        (table.get(1).unwrap().runs, table.get(1).unwrap().busy_ticks),
        (1, 1)
    );
}

#[test]
fn a_task_that_ends_leaves_and_the_rest_move_up() {
    let mut table = TaskTable::new();
    table.add(1);
    table.add(2);
    table.add(3);
    table.start_running(3, 0);
    table.name_running("bluetooth");
    table.stop_running(1);

    table.start_running(1, 2);
    table.remove(1);
    table.stop_running(5);
    table.remove(99);

    assert_eq!(table.len(), 2);
    assert_eq!(table.get(1).unwrap().name(), "bluetooth");
    assert_eq!(table.get(1).unwrap().runs, 1);
    assert_eq!(table.get(2), None);
}

#[test]
fn polls_of_unknown_tasks_and_tasks_past_the_room_are_left_out() {
    let mut table = TaskTable::new();
    assert!(table.is_empty());
    for id in 0..MOST_TASKS as u32 + 2 {
        table.add(id + 1);
    }

    table.start_running(500, 0);
    table.name_running("lost");
    table.stop_running(9);

    assert_eq!(table.len(), MOST_TASKS);
    assert!((0..MOST_TASKS).all(|place| table.get(place).unwrap().runs == 0));
    assert!((0..MOST_TASKS).all(|place| table.get(place).unwrap().name() == UNNAMED));
}

#[test]
fn busy_time_counts_across_the_clock_going_round() {
    let mut table = TaskTable::new();
    table.add(1);

    table.start_running(1, u32::MAX - 1);
    table.stop_running(3);

    assert_eq!(table.get(0).unwrap().busy_ticks, 5);
}

#[test]
fn ticks_turn_into_whole_milliseconds() {
    assert_eq!(ticks_to_milliseconds(32_768, 32_768), 1000);
    assert_eq!(ticks_to_milliseconds(33, 32_768), 1);
    assert_eq!(ticks_to_milliseconds(32, 32_768), 0);
}
