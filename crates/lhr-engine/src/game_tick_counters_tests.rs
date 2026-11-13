//! Counter fragment over the retained game state and its live clock readers.
use super::*;

#[test]
fn tick_advances_all_counters_and_zero_delta_still_counts_a_frame() {
    let mut game = GameInlineState::default();
    let clock = game.game_clock();
    assert_eq!(game.advance_tick_counters(16), 16);
    assert_eq!(clock.read(), Some(16));
    assert_eq!((game.game_frame_count(), game.get_init_frames()), (1, 1));
    assert_eq!(game.advance_tick_counters(0), 16);
    assert_eq!(clock.read(), Some(16));
    assert_eq!((game.game_frame_count(), game.get_init_frames()), (2, 2));
}

#[test]
fn tick_counters_wrap_at_their_evidenced_32_bit_widths() {
    let mut game = GameInlineState::default();
    game.set_game_time(i32::MAX);
    game.game_frames = i32::MAX;
    game.set_init_frames(u32::MAX);
    assert_eq!(game.advance_tick_counters(1), i32::MIN);
    assert_eq!(game.game_frame_count(), i32::MIN);
    assert_eq!(game.get_init_frames(), 0);
    assert_eq!(game.advance_tick_counters(u32::MAX), i32::MAX);
    assert_eq!(game.game_frame_count(), i32::MIN + 1);
    assert_eq!(game.get_init_frames(), 1);
}

#[test]
fn clock_staging_alone_does_not_count_a_frame() {
    let mut game = GameInlineState::default();
    game.set_game_time(123);
    assert_eq!((game.game_frame_count(), game.get_init_frames()), (0, 0));
    assert_eq!(game.advance_tick_counters(7), 130);
    assert_eq!((game.game_frame_count(), game.get_init_frames()), (1, 1));
}

#[test]
fn tick_does_not_draw_or_reseed_the_shared_random_stream() {
    let mut game = GameInlineState::default();
    let mut random = game.random_source();
    assert_eq!(random.next_raw(), Some(41));
    game.advance_tick_counters(16);
    game.advance_tick_counters(0);
    assert_eq!(random.next_raw(), Some(18467));
}
