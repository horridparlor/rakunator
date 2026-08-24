use crate::playback;
use crate::waveform::Waveform;
use std::time::Duration;

pub const WAVEFORM: Waveform = Waveform::Triangle;
pub const FREQUENCY_HZ: f32 = 440.0;
pub const DURATION: Duration = Duration::from_secs(2);

pub fn say_hello() {
    println!("playing triangle wave");
    playback::play_wave(FREQUENCY_HZ, DURATION, move |phase| WAVEFORM.sample(phase));
}