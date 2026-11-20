//! A Rust bot plays a real map in this process.

use std::sync::Mutex;

use bwapi::{Bot, Context};

#[derive(Debug, Default, PartialEq)]
struct Seen {
    width: i32,
    height: i32,
    starts: usize,
    frames: i32,
    ended: bool,
}

static SEEN: Mutex<Option<Seen>> = Mutex::new(None);

struct Recorder;

impl<'game> Bot<'game> for Recorder {
    fn on_start(cx: Context<'_, 'game>) -> Self {
        let game = cx.game();
        *SEEN.lock().unwrap() = Some(Seen {
            width: game.map_width(),
            height: game.map_height(),
            starts: game.start_locations().len(),
            ..Seen::default()
        });
        Recorder
    }

    fn on_frame(&mut self, _cx: Context<'_, 'game>) {
        SEEN.lock().unwrap().as_mut().unwrap().frames += 1;
    }

    fn on_end(self, _cx: Context<'_, 'game>, _is_winner: bool) {
        SEEN.lock().unwrap().as_mut().unwrap().ended = true;
    }
}

bwapi::bot!(Recorder);

#[test]
fn a_bot_plays_showdown() {
    let map = openbw_host::map("scai/(2)Showdown.scx");
    openbw_host::play(&map, 10, gameInit, newAIModule).unwrap();

    let seen = SEEN.lock().unwrap().take().expect("the bot never started");
    assert_eq!((seen.width, seen.height, seen.starts), (64, 192, 2));
    assert!(seen.frames > 0, "no frames: {seen:?}");
    assert!(seen.ended, "no on_end: {seen:?}");
}
