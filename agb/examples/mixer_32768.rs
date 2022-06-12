#![no_std]
#![no_main]

use agb::sound::mixer::SoundChannel;
use agb::{include_wav, Gba};

// Music - "Crazy glue" by Josh Woodward, free download at http://joshwoodward.com
const LET_IT_IN: &[u8] = include_wav!("examples/JoshWoodward-CrazyGlue.wav");

#[agb::entry]
fn main(mut gba: Gba) -> ! {
    let vblank_provider = agb::interrupt::VBlank::get();

    let timer_controller = gba.timers.timers();
    let mut timer = timer_controller.timer2;
    timer.set_enabled(true);

    let mut mixer = gba.mixer.mixer();
    mixer.enable();

    let mut channel = SoundChannel::new(LET_IT_IN);
    channel.stereo();
    mixer.play_sound(channel).unwrap();

    let _interrupt = mixer.setup_interrupt_handler();

    let mut frame_counter = 0i32;
    loop {
        vblank_provider.wait_for_vblank();
        let before_mixing_cycles = timer.value();
        mixer.frame();
        let after_mixing_cycles = timer.value();

        frame_counter = frame_counter.wrapping_add(1);

        if frame_counter % 128 == 0 {
            let total_cycles = after_mixing_cycles.wrapping_sub(before_mixing_cycles) as u32;

            let percent = (total_cycles * 100) / 280896;
            agb::println!(
                "Took {} cycles to calculate mixer ~= {}% of total frame",
                total_cycles,
                percent
            );
        }
    }
}