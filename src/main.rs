mod export;
mod playback;
mod tests;
mod waveform;

use std::io::{self, Write};

fn main() {
    println!("Choose a waveform to play:");
    println!("  1) Sine");
    println!("  2) Square");
    println!("  3) Triangle");
    println!("  4) Sawtooth");
    print!("> ");
    io::stdout().flush().expect("failed to flush stdout");

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .expect("failed to read input");

    let selection = match input.trim() {
        "1" | "sine" | "sin" => {
            tests::sin_test::say_hello();
            Some((
                tests::sin_test::WAVEFORM,
                tests::sin_test::FREQUENCY_HZ,
                tests::sin_test::DURATION,
            ))
        }
        "2" | "square" => {
            tests::square_test::say_hello();
            Some((
                tests::square_test::WAVEFORM,
                tests::square_test::FREQUENCY_HZ,
                tests::square_test::DURATION,
            ))
        }
        "3" | "triangle" => {
            tests::triangle_test::say_hello();
            Some((
                tests::triangle_test::WAVEFORM,
                tests::triangle_test::FREQUENCY_HZ,
                tests::triangle_test::DURATION,
            ))
        }
        "4" | "sawtooth" | "saw" => {
            tests::sawtooth_test::say_hello();
            Some((
                tests::sawtooth_test::WAVEFORM,
                tests::sawtooth_test::FREQUENCY_HZ,
                tests::sawtooth_test::DURATION,
            ))
        }
        other => {
            println!("unrecognized choice: {other}");
            None
        }
    };

    let Some((waveform, frequency_hz, duration)) = selection else {
        return;
    };

    print!("Create .wav and .mp3 files of this in ~/Downloads? [y/N] ");
    io::stdout().flush().expect("failed to flush stdout");

    let mut answer = String::new();
    io::stdin()
        .read_line(&mut answer)
        .expect("failed to read input");

    if matches!(answer.trim().to_lowercase().as_str(), "y" | "yes") {
        export::export_wave(waveform, frequency_hz, duration);
    }
}