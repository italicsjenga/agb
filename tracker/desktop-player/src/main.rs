use std::{env, error::Error, fs, path::Path, sync::mpsc};

use cpal::{
    traits::{DeviceTrait, HostTrait, StreamTrait},
    SampleFormat, SampleRate,
};
use mixer::Mixer;
use xmrs::{module::Module, xm::xmmodule::XmModule};

mod mixer;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = env::args().collect();

    let file_path = &args[1];
    let module = load_module_from_file(Path::new(file_path))?;

    let track = agb_xm_core::parse_module(&module);

    let mut mixer = Mixer::new();
    let mut tracker = agb_tracker::TrackerInner::new(&track);

    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .expect("Failed to open output device");

    let mut supported_configs = device.supported_output_configs()?;
    let config = supported_configs
        .find_map(|config| {
            if config.channels() == 2 && config.sample_format() == SampleFormat::F32 {
                return config.try_with_sample_rate(SampleRate(32768));
            }

            None
        })
        .expect("Could not produce valid config");

    let (tx, rx) = mpsc::sync_channel(32768 * 3);

    let stream = device.build_output_stream(
        &config.into(),
        move |data: &mut [f32], _| {
            for val in data.iter_mut() {
                *val = rx.recv().unwrap();
            }
        },
        |err| eprintln!("Error on audio stream {err}"),
        None,
    )?;

    stream.play()?;

    loop {
        tracker.step(&mut mixer);
        for (l, r) in mixer.frame() {
            tx.send((l as f32) / 128.0)?;
            tx.send((r as f32) / 128.0)?;
        }
    }
}

fn load_module_from_file(xm_path: &Path) -> Result<Module, Box<dyn Error>> {
    let file_content = fs::read(xm_path)?;
    Ok(XmModule::load(&file_content)?.to_module())
}
