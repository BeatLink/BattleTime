use super::*;

// ============================================================================
// Settings
// ============================================================================

#[test]
fn every_setting_starts_at_its_default() {
    let settings = Settings::default();

    for setting in Setting::ALL {
        assert_eq!(settings.get(setting), setting.limits().default);
    }
    assert!(settings.is_on(Setting::Uses24HourClock));
    assert_eq!(settings.get(Setting::StepGoal), 10_000);
    assert_eq!(settings.get(Setting::ScreenTimeoutSeconds), 15);
}

#[test]
fn numbers_and_names_go_both_ways() {
    for (place, setting) in Setting::ALL.into_iter().enumerate() {
        assert_eq!(setting.number() as usize, place);
        assert_eq!(Setting::from_number(setting.number()), Some(setting));
        assert_eq!(Setting::from_name(setting.name()), Some(setting));
    }
    assert_eq!(Setting::from_number(Setting::COUNT as u8), None);
    assert_eq!(Setting::from_name("volume"), None);
}

#[test]
fn defaults_lie_inside_their_limits() {
    for setting in Setting::ALL {
        let limits = setting.limits();
        assert!(
            limits.lowest <= limits.default && limits.default <= limits.highest,
            "{setting:?}"
        );
    }
}

#[test]
fn setting_reports_whether_the_value_changed() {
    let mut settings = Settings::default();

    assert_eq!(settings.set(Setting::Brightness, 3), Ok(true));
    assert_eq!(settings.set(Setting::Brightness, 3), Ok(false));
    assert_eq!(settings.get(Setting::Brightness), 3);
}

#[test]
fn values_outside_the_limits_are_refused() {
    let mut settings = Settings::default();

    assert_eq!(
        settings.set(Setting::Brightness, 4),
        Err(SettingError::OutOfLimits {
            lowest: 1,
            highest: 3
        })
    );
    assert_eq!(settings, Settings::default());
}

#[test]
fn apps_set_settings_by_name() {
    let mut settings = Settings::default();

    assert_eq!(settings.set_by_name("step_goal", 8_000), Ok(Setting::StepGoal));
    assert_eq!(settings.get(Setting::StepGoal), 8_000);
    assert_eq!(
        settings.set_by_name("volume", 1),
        Err(SettingError::UnknownSetting)
    );
    assert!(settings.set_by_name("step_goal", i64::MAX).is_err());
    assert_eq!(settings.get(Setting::StepGoal), 8_000);
}

// ============================================================================
// Records
// ============================================================================

#[test]
fn records_hold_the_number_then_the_value_little_endian() {
    let mut settings = Settings::default();
    settings.set(Setting::StepGoal, 12_345).unwrap();
    let mut buffer = [0; ALL_RECORDS_BYTES];

    settings.write_records(&mut buffer);

    let step_goal = &buffer[2 * RECORD_BYTES..3 * RECORD_BYTES];
    assert_eq!(step_goal, &[2, 0x39, 0x30, 0, 0]);
}

#[test]
fn records_written_are_read_back_the_same() {
    let mut settings = Settings::default();
    settings.set(Setting::Uses24HourClock, 0).unwrap();
    settings.set(Setting::Chimes, chimes::EVERY_HALF_HOUR).unwrap();
    settings.set(Setting::StepGoal, 7_500).unwrap();
    let mut buffer = [0; ALL_RECORDS_BYTES];
    settings.write_records(&mut buffer);

    let mut read_back = Settings::default();
    let changed = read_back.apply_records(&buffer);

    assert_eq!(read_back, settings);
    assert_eq!(
        changed,
        Setting::Uses24HourClock.bit() | Setting::Chimes.bit() | Setting::StepGoal.bit()
    );
}

#[test]
fn a_write_may_carry_just_one_record() {
    let mut settings = Settings::default();

    let changed = settings.apply_records(&[3, 1, 0, 0, 0]);

    assert_eq!(changed, Setting::Brightness.bit());
    assert_eq!(settings.get(Setting::Brightness), 1);
}

#[test]
fn unknown_settings_and_bad_values_are_skipped() {
    let mut settings = Settings::default();
    let records = [
        200, 1, 0, 0, 0, // a setting from a newer firmware
        3, 9, 0, 0, 0, // brightness 9, outside its limits
        7, 1, 0, 0, 0, // do not disturb on
        4, 30, // a record cut short
    ];

    let changed = settings.apply_records(&records);

    assert_eq!(changed, Setting::DoNotDisturb.bit());
    assert_eq!(settings.get(Setting::Brightness), 2);
    assert_eq!(settings.get(Setting::ScreenTimeoutSeconds), 15);
}

#[test]
fn the_file_round_trips() {
    let mut settings = Settings::default();
    settings.set(Setting::AlwaysOnScreen, 1).unwrap();
    settings.set(Setting::ScreenTimeoutSeconds, 30).unwrap();
    let mut buffer = [0; FILE_BYTES];

    let length = settings.write_file(&mut buffer);

    assert_eq!(buffer[0], FILE_FORMAT);
    assert_eq!(Settings::from_file(&buffer[..length]), settings);
}

#[test]
fn a_missing_or_foreign_file_gives_the_defaults() {
    assert_eq!(Settings::from_file(&[]), Settings::default());
    assert_eq!(Settings::from_file(&[99, 3, 1, 0, 0, 0]), Settings::default());
}

#[test]
fn bluetooth_turns_on_again_after_a_restart() {
    let mut settings = Settings::default();
    settings.set(Setting::Bluetooth, 0).unwrap();
    settings.set(Setting::AllowsUpdates, 0).unwrap();
    let mut buffer = [0; FILE_BYTES];

    let length = settings.write_file(&mut buffer);
    let read_back = Settings::from_file(&buffer[..length]);

    assert!(read_back.is_on(Setting::Bluetooth));
    assert!(!read_back.is_on(Setting::AllowsUpdates));
}

#[test]
fn a_file_from_before_the_newer_settings_reads_them_as_their_defaults() {
    let old_file = [FILE_FORMAT, 7, 1, 0, 0, 0];

    let settings = Settings::from_file(&old_file);

    assert!(settings.is_on(Setting::DoNotDisturb));
    assert!(settings.is_on(Setting::AllowsUpdates));
    assert!(settings.is_on(Setting::AlarmsInDoNotDisturb));
    assert_eq!(settings.get(Setting::WakeOnTouch), wake_on_touch::TAP);
    assert_eq!(settings.get(Setting::HeartRateInterval), heart_rate_interval::OFF);
    assert!(!settings.is_on(Setting::RaiseToWake));
    assert!(!settings.is_on(Setting::LowerToSleep));
    assert!(!settings.is_on(Setting::ShakeToWake));
    assert_eq!(settings.get(Setting::ShakeThreshold), shake_threshold::DEFAULT);
    assert!(!settings.is_sleep_mode());
    assert_eq!(settings.get(Setting::WatchFace), watch_face::NONE_CHOSEN);
}

#[test]
fn the_watch_face_takes_any_positive_face_number_and_survives_a_restart() {
    let mut settings = Settings::default();

    assert!(settings.set(Setting::WatchFace, -1).is_err());
    assert_eq!(
        settings.set_by_name("watch_face", 0x3114_E811),
        Ok(Setting::WatchFace)
    );
    let mut file = [0; FILE_BYTES];
    settings.write_file(&mut file);

    assert_eq!(Settings::from_file(&file).get(Setting::WatchFace), 0x3114_E811);
    assert_eq!(Setting::from_number(18), Some(Setting::WatchFace));
}

// ============================================================================
// Sleep mode
// ============================================================================

#[test]
fn sleep_mode_turns_do_not_disturb_on_with_it() {
    let mut settings = Settings::default();
    let before = settings;

    assert_eq!(settings.set(Setting::SleepMode, 1), Ok(true));

    assert!(settings.is_sleep_mode());
    assert!(settings.is_on(Setting::DoNotDisturb));
    assert_eq!(
        settings.differences_from(&before),
        Setting::SleepMode.bit() | Setting::DoNotDisturb.bit()
    );
}

#[test]
fn turning_do_not_disturb_off_ends_sleep_mode() {
    let mut settings = Settings::default();
    settings.set(Setting::SleepMode, 1).unwrap();

    settings.set(Setting::DoNotDisturb, 0).unwrap();

    assert!(!settings.is_sleep_mode());
    assert!(!settings.is_on(Setting::DoNotDisturb));
}

#[test]
fn ending_sleep_mode_leaves_do_not_disturb_on() {
    let mut settings = Settings::default();
    settings.set(Setting::SleepMode, 1).unwrap();

    settings.set(Setting::SleepMode, 0).unwrap();

    assert!(!settings.is_sleep_mode());
    assert!(settings.is_on(Setting::DoNotDisturb));
}

#[test]
fn records_report_the_settings_sleep_mode_changed_with_it() {
    let mut settings = Settings::default();

    let changed = settings.apply_records(&[17, 1, 0, 0, 0]);

    assert_eq!(changed, Setting::SleepMode.bit() | Setting::DoNotDisturb.bit());
}

#[test]
fn the_shake_threshold_keeps_to_infinitimes_range() {
    let mut settings = Settings::default();

    assert!(settings.set(Setting::ShakeThreshold, 4096).is_err());
    assert_eq!(
        settings.set_by_name("shake_threshold", 900),
        Ok(Setting::ShakeThreshold)
    );
    assert_eq!(settings.get(Setting::ShakeThreshold), 900);
}

#[test]
fn the_heart_rate_interval_gives_seconds_only_for_measuring_now_and_then() {
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::OFF),
        None
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::CONTINUOUS),
        None
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::EVERY_30_SECONDS),
        Some(30)
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::EVERY_MINUTE),
        Some(60)
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::EVERY_5_MINUTES),
        Some(300)
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::EVERY_10_MINUTES),
        Some(600)
    );
    assert_eq!(
        heart_rate_interval::seconds_between(heart_rate_interval::EVERY_30_MINUTES),
        Some(1800)
    );
}

// ============================================================================
// The screen timeout
// ============================================================================

#[test]
fn the_light_lowers_two_seconds_before_the_timeout() {
    assert_eq!(screen_timeout_stage(0, 15_000), ScreenTimeoutStage::Lit);
    assert_eq!(screen_timeout_stage(12_999, 15_000), ScreenTimeoutStage::Lit);
    assert_eq!(
        screen_timeout_stage(13_000, 15_000),
        ScreenTimeoutStage::LightLowered
    );
    assert_eq!(
        screen_timeout_stage(14_999, 15_000),
        ScreenTimeoutStage::LightLowered
    );
    assert_eq!(screen_timeout_stage(15_000, 15_000), ScreenTimeoutStage::TimedOut);
    assert_eq!(screen_timeout_stage(90_000, 15_000), ScreenTimeoutStage::TimedOut);
}

#[test]
fn a_short_timeout_lowers_the_light_for_its_second_half_at_most() {
    assert_eq!(screen_timeout_stage(1_499, 3_000), ScreenTimeoutStage::Lit);
    assert_eq!(
        screen_timeout_stage(1_500, 3_000),
        ScreenTimeoutStage::LightLowered
    );
    assert_eq!(next_screen_timeout_step_in_milliseconds(0, 3_000), 1_500);
}

#[test]
fn the_next_step_is_the_lowering_then_the_timeout() {
    assert_eq!(next_screen_timeout_step_in_milliseconds(0, 15_000), 13_000);
    assert_eq!(next_screen_timeout_step_in_milliseconds(12_999, 15_000), 13_000);
    assert_eq!(next_screen_timeout_step_in_milliseconds(13_000, 15_000), 15_000);
    assert_eq!(next_screen_timeout_step_in_milliseconds(20_000, 15_000), 15_000);
}

#[test]
fn the_shortest_setting_still_lowers_the_light_two_seconds_early() {
    let shortest_in_milliseconds = Setting::ScreenTimeoutSeconds.limits().lowest as u64 * 1000;

    assert_eq!(
        next_screen_timeout_step_in_milliseconds(0, shortest_in_milliseconds),
        shortest_in_milliseconds - LOWER_LIGHT_BEFORE_TIMEOUT_IN_MILLISECONDS
    );
}
