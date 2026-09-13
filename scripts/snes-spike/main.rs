//! Development-only probe for the pinned upstream checkout; not an app dependency.
use jgenesis_common::frontend::*;
use snes_core::{api::{SnesEmulator, SnesEmulatorConfig, CoprocessorRoms}, input::SnesInputs};
use std::{collections::HashMap, io, time::Instant};

fn state_config() -> impl bincode::config::Config {
    // Match upstream's native driver's state configuration, including its cap.
    bincode::config::standard().with_little_endian().with_fixed_int_encoding().with_limit::<{100 * 1024 * 1024}>()
}

#[derive(Default)]
struct Video { pixels: Vec<Color>, dimensions: (u32, u32) }
impl Renderer for Video {
    type Err = io::Error;
    fn render_frame(&mut self, pixels: &[Color], size: FrameSize, _: f64, _: RenderFrameOptions) -> io::Result<()> {
        self.pixels.clear();
        self.pixels.extend_from_slice(&pixels[..(size.width * size.height) as usize]);
        self.dimensions = (size.width, size.height);
        Ok(())
    }
}
#[derive(Default)]
struct Audio(Vec<(f64, f64)>);
impl AudioOutput for Audio {
    type Err = io::Error;
    fn push_sample(&mut self, l: f64, r: f64) -> io::Result<()> { self.0.push((l, r)); Ok(()) }
}
#[derive(Default)]
struct Saves(HashMap<String, Vec<u8>>);
impl SaveWriter for Saves {
    type Err = io::Error;
    fn load_bytes(&mut self, ext: &str) -> io::Result<Vec<u8>> {
        self.0.get(ext).cloned().ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
    }
    fn persist_bytes(&mut self, ext: &str, bytes: &[u8]) -> io::Result<()> { self.0.insert(ext.into(), bytes.into()); Ok(()) }
    fn load_serialized<D: bincode::Decode<()>>(&mut self, ext: &str) -> io::Result<D> {
        bincode::decode_from_slice(&self.load_bytes(ext)?, state_config()).map(|v|v.0).map_err(io::Error::other)
    }
    fn persist_serialized<E: bincode::Encode>(&mut self, ext: &str, value: E) -> io::Result<()> {
        self.persist_bytes(ext, &bincode::encode_to_vec(value, state_config()).map_err(io::Error::other)?)
    }
}
fn rom() -> Vec<u8> {
    let mut bytes = vec![0; 32768];
    let program = [0x78,0xa9,0x0f,0x8d,0x00,0x21,0xe6,0x00,0xa5,0x00,0x9c,0x21,0x21,0x8d,0x22,0x21,0x9c,0x22,0x21,0x80,0xf1];
    bytes[..program.len()].copy_from_slice(&program);
    bytes[0x7fc0..0x7fd5].copy_from_slice(b"EMULIA GENERATED TEST");
    bytes[0x7fd5]=0x20; bytes[0x7fd7]=5; bytes[0x7fd9]=1;
    bytes[0x7fdc]=0xff; bytes[0x7fdd]=0xff;
    bytes[0x7ffc]=0; bytes[0x7ffd]=0x80;
    bytes
}
fn frame(core: &mut SnesEmulator, video: &mut Video, audio: &mut Audio, saves: &mut Saves) {
    audio.0.clear();
    let inputs = SnesInputs::default();
    for _ in 0..2_000_000 {
        if core.tick(video, audio, &mut ConstantInputPoller(&inputs), saves).unwrap() == TickEffect::FrameRendered { return; }
    }
    panic!("Frame did not complete within tick budget");
}
fn snapshot(core: &SnesEmulator) -> Vec<u8> {
    bincode::encode_to_vec(core.to_save_state(), state_config()).unwrap()
}
fn main() {
    let mut saves = Saves::default();
    let mut core = SnesEmulator::create(rom(), SnesEmulatorConfig::default(), CoprocessorRoms::none(), &mut saves).unwrap();
    core.update_audio_output_frequency(48000);
    let (mut video, mut audio) = (Video::default(), Audio::default());
    for _ in 0..10 { frame(&mut core, &mut video, &mut audio, &mut saves); }
    let saved = snapshot(&core);
    for _ in 0..10 { frame(&mut core, &mut video, &mut audio, &mut saves); }
    let expected = snapshot(&core);
    let picture = video.pixels.clone(); let sound = audio.0.clone();
    let (decoded, used) = bincode::decode_from_slice(&saved, state_config()).unwrap();
    assert_eq!(used, saved.len()); core.load_state(decoded);
    for _ in 0..10 { frame(&mut core, &mut video, &mut audio, &mut saves); }
    assert_eq!(expected, snapshot(&core)); assert_eq!(picture, video.pixels); assert_eq!(sound, audio.0);
    let plain_start = Instant::now();
    for _ in 0..120 { frame(&mut core, &mut video, &mut audio, &mut saves); }
    let plain_fps = 120.0 / plain_start.elapsed().as_secs_f64();
    let mut history = emulation_runtime::history::Rewind::new(64 * 1024 * 1024);
    history.push(&snapshot(&core));
    let start = Instant::now();
    let mut minimum = usize::MAX; let mut maximum = 0;
    for _ in 0..120 {
        frame(&mut core, &mut video, &mut audio, &mut saves);
        let state = snapshot(&core); minimum=minimum.min(state.len()); maximum=maximum.max(state.len()); history.push(&state);
    }
    let elapsed=start.elapsed();
    let depth=history.depth(); let budget=history.bytes();
    let mut restored=0;
    while history.restore(|state| -> io::Result<()> {
        let (decoded, used)=bincode::decode_from_slice(state,state_config()).map_err(io::Error::other)?;
        assert_eq!(used,state.len()); core.load_state(decoded); Ok(())
    }).unwrap() { restored+=1; }
    assert_eq!(restored,depth);
    println!("frame={}x{} audio_frames={} snapshot={}..{} bytes replay=PASS history_depth={} history_accounted={} benchmark_frames=120 seconds={:.6} fps={:.2} plain_fps={:.2}",
        video.dimensions.0,video.dimensions.1,audio.0.len(),minimum,maximum,depth,budget,elapsed.as_secs_f64(),120.0/elapsed.as_secs_f64(),plain_fps);
}
