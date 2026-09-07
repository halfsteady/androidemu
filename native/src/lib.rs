//! Android JNI boundary. The machine has one owner and never runs in the audio callback.
use jni::{
    objects::{JByteArray, JByteBuffer, JClass},
    sys::{jboolean, jbyteArray, jfloat, jfloatArray, jint, jobjectArray, jstring},
    JNIEnv,
};
use nes_core::{rewind::Rewind, Buttons, Nes};
use std::sync::Mutex;
static MACHINE: Mutex<Option<Nes>> = Mutex::new(None);
/// Bounded in bytes rather than frames: see `nes_core::rewind`. 64 MB buys
/// roughly a minute of a mostly still screen and rather less of a scrolling
/// one, which is the trade worth making automatically.
const REWIND_BUDGET: usize = 64 * 1024 * 1024;
static REWIND: Mutex<Option<Rewind>> = Mutex::new(None);
/// Start a fresh chain anchored on where the machine is now. The anchor is
/// always the machine's current state, so one pop is exactly one frame back.
fn rewind_reset(nes: &Nes) {
    let mut chain = Rewind::new(REWIND_BUDGET);
    chain.push(&nes.save_state());
    *REWIND.lock().unwrap() = Some(chain);
}
/// Paint the current framebuffer into a caller-owned direct buffer.
///
/// # Safety
/// `address` must point to at least `256 * 240 * 4` writable bytes, which the
/// caller establishes from the direct buffer's capacity.
unsafe fn blit(nes: &Nes, address: *mut u8) {
    let out = unsafe { std::slice::from_raw_parts_mut(address, 256 * 240 * 4) };
    // Read the table once. It is behind a lock because the shell can change it
    // between frames, and taking that lock per pixel would be absurd.
    let palette = CHOSEN_PALETTE.lock().unwrap().unwrap_or(PALETTE);
    for (pixel, &index) in out.as_chunks_mut::<4>().0.iter_mut().zip(nes.framebuffer()) {
        let rgb = palette[(index & 63) as usize];
        pixel.copy_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
    }
}

/// The palette in force, or `None` for the one built in below.
///
/// The framebuffer the core produces is 6-bit indices, not colour, so which
/// colours those indices mean is a presentation choice and belongs to whoever is
/// holding the tablet. Changing it costs nothing: the lookup was already
/// happening, this only changes what it reads.
static CHOSEN_PALETTE: Mutex<Option<[u32; 64]>> = Mutex::new(None);

/// Set the 64 colours the framebuffer's indices mean, as 192 bytes of RGB.
/// An empty array restores the built-in table.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_setPalette(
    mut env: JNIEnv,
    _: JClass,
    colors: JByteArray,
) {
    let Ok(bytes) = env.convert_byte_array(&colors) else {
        error(&mut env, "Palette could not be read");
        return;
    };
    if bytes.is_empty() {
        *CHOSEN_PALETTE.lock().unwrap() = None;
        return;
    }
    if bytes.len() != 64 * 3 {
        error(&mut env, "A palette is 64 colours, so 192 bytes");
        return;
    }
    let mut table = [0u32; 64];
    for (entry, rgb) in table.iter_mut().zip(bytes.as_chunks::<3>().0) {
        *entry = (rgb[0] as u32) << 16 | (rgb[1] as u32) << 8 | rgb[2] as u32;
    }
    *CHOSEN_PALETTE.lock().unwrap() = Some(table);
}

/// Paint the current frame again without advancing the machine.
///
/// Changing the palette while paused has to show up, and re-running a frame to
/// see it would move the game. The framebuffer is still there; only the table
/// read against it has changed.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_repaint(mut env: JNIEnv, _: JClass, buffer: JByteBuffer) {
    let capacity = env.get_direct_buffer_capacity(&buffer).unwrap_or(0);
    if capacity < 256 * 240 * 4 {
        error(&mut env, "Invalid video buffer");
        return;
    }
    let Ok(address) = env.get_direct_buffer_address(&buffer) else {
        error(&mut env, "Video buffer must be direct");
        return;
    };
    let machine = MACHINE.lock().unwrap();
    let Some(nes) = machine.as_ref() else { return };
    // JNI verified the direct buffer capacity; Kotlin retains it for this call.
    unsafe { blit(nes, address) };
}
#[cfg(target_os = "android")]
mod audio;
fn error(env: &mut JNIEnv, message: impl ToString) {
    let _ = env.throw_new("java/lang/IllegalStateException", message.to_string());
}
/// The table used when nothing else has been chosen.
const PALETTE: [u32; 64] = [
    0x666666, 0x002a88, 0x1412a7, 0x3b00a4, 0x5c007e, 0x6e0040, 0x6c0600, 0x561d00, 0x333500,
    0x0b4800, 0x005200, 0x004f08, 0x00404d, 0, 0, 0, 0xadadad, 0x155fd9, 0x4240ff, 0x7527fe,
    0xa01acc, 0xb71e7b, 0xb53120, 0x994e00, 0x6b6d00, 0x388700, 0x0c9300, 0x008f32, 0x007c8d, 0, 0,
    0, 0xfffeff, 0x64b0ff, 0x9290ff, 0xc676ff, 0xf36aff, 0xfe6ecc, 0xfe8170, 0xea9e22, 0xbcbe00,
    0x88d800, 0x5ce430, 0x45e082, 0x48cdde, 0x4f4f4f, 0, 0, 0xfffeff, 0xc0dfff, 0xd3d2ff, 0xe8c8ff,
    0xfbc2ff, 0xfec4ea, 0xfeccc5, 0xf7d8a5, 0xe4e594, 0xcfef96, 0xbdf4ab, 0xb3f3cc, 0xb5ebf2,
    0xb8b8b8, 0, 0,
];
/// What the loaded ROM's header had to have corrected, one plain sentence each.
///
/// A repaired header is usually the reason a game looks wrong, so the app keeps
/// these in its problem log rather than fixing things silently.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_headerNotes(
    mut env: JNIEnv,
    _: JClass,
) -> jobjectArray {
    let notes: Vec<&'static str> = match MACHINE.lock().unwrap().as_ref() {
        Some(nes) => nes.bus.cart.header.fixes.labels().collect(),
        None => Vec::new(),
    };
    let class = match env.find_class("java/lang/String") {
        Ok(class) => class,
        Err(e) => {
            error(&mut env, e.to_string());
            return std::ptr::null_mut();
        }
    };
    let empty = match env.new_string("") {
        Ok(s) => s,
        Err(e) => {
            error(&mut env, e.to_string());
            return std::ptr::null_mut();
        }
    };
    let array = match env.new_object_array(notes.len() as i32, &class, &empty) {
        Ok(array) => array,
        Err(e) => {
            error(&mut env, e.to_string());
            return std::ptr::null_mut();
        }
    };
    for (at, note) in notes.iter().enumerate() {
        match env.new_string(note).and_then(|s| env.set_object_array_element(&array, at as i32, s)) {
            Ok(()) => {}
            Err(e) => {
                error(&mut env, e.to_string());
                return std::ptr::null_mut();
            }
        }
    }
    array.into_raw()
}

#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_load(
    mut env: JNIEnv,
    _: JClass,
    rom: JByteArray,
) -> jstring {
    let result = env
        .convert_byte_array(&rom)
        .map_err(|e| e.to_string())
        .and_then(|bytes| Nes::new(&bytes).map_err(|e| e.to_string()));
    match result {
        Ok(nes) => {
            let id = format!("{:016x}", nes.bus.cart.header.hash);
            rewind_reset(&nes);
            *MACHINE.lock().unwrap() = Some(nes);
            env.new_string(id).unwrap().into_raw()
        }
        Err(e) => {
            error(&mut env, e);
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_frame(
    mut env: JNIEnv,
    _: JClass,
    buffer: JByteBuffer,
    p1: jint,
    p2: jint,
    advance: jboolean,
) {
    let capacity = env.get_direct_buffer_capacity(&buffer).unwrap_or(0);
    if capacity < 256 * 240 * 4 {
        error(&mut env, "Invalid video buffer");
        return;
    }
    let Ok(address) = env.get_direct_buffer_address(&buffer) else {
        error(&mut env, "Video buffer must be direct");
        return;
    };
    let mut machine = MACHINE.lock().unwrap();
    let Some(nes) = machine.as_mut() else {
        return;
    };
    if advance != 0 {
        nes.set_buttons(0, Buttons(p1 as u8));
        nes.set_buttons(1, Buttons(p2 as u8));
        nes.step_frame();
        // Recorded after the frame, so the chain's anchor is where the machine
        // actually is and the first step back is one frame, not two.
        if let Some(rewind) = REWIND.lock().unwrap().as_mut() {
            rewind.push(&nes.save_state());
        }
        #[cfg(target_os = "android")]
        audio::push(nes.bus.apu.samples());
    }
    // JNI verified the direct buffer capacity; Kotlin retains it for this call.
    unsafe { blit(nes, address) };
}

/// Step one frame back and paint it. Returns false once the chain runs out, so
/// the shell can stop asking and tell the player they are as far back as it goes.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_rewind(
    mut env: JNIEnv,
    _: JClass,
    buffer: JByteBuffer,
) -> jboolean {
    let capacity = env.get_direct_buffer_capacity(&buffer).unwrap_or(0);
    if capacity < 256 * 240 * 4 {
        error(&mut env, "Invalid video buffer");
        return 0;
    }
    let Ok(address) = env.get_direct_buffer_address(&buffer) else {
        error(&mut env, "Video buffer must be direct");
        return 0;
    };
    let mut machine = MACHINE.lock().unwrap();
    let Some(nes) = machine.as_mut() else {
        return 0;
    };
    let mut chain = REWIND.lock().unwrap();
    let stepped = match chain.as_mut().and_then(|rewind| rewind.pop()) {
        // A state that came out of this same machine a moment ago should always
        // load. If it somehow does not, leave the machine untouched - load_state
        // is transactional - and report that rewinding stopped.
        Some(state) => nes.load_state(state).is_ok(),
        None => false,
    };
    drop(chain);
    unsafe { blit(nes, address) };
    stepped as jboolean
}

/// Frames the chain can still give back.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_rewindDepth(_: JNIEnv, _: JClass) -> jint {
    REWIND.lock().unwrap().as_ref().map_or(0, |rewind| rewind.depth().min(i32::MAX as usize) as jint)
}
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_snapshot(
    mut env: JNIEnv,
    _: JClass,
    battery: jboolean,
) -> jbyteArray {
    let machine = MACHINE.lock().unwrap();
    let Some(nes) = machine.as_ref() else {
        error(&mut env, "No game is open");
        return std::ptr::null_mut();
    };
    let bytes = if battery != 0 {
        nes.battery_ram().unwrap_or(&[]).to_vec()
    } else {
        nes.save_state()
    };
    match env.byte_array_from_slice(&bytes) {
        Ok(a) => a.into_raw(),
        Err(e) => {
            error(&mut env, e);
            std::ptr::null_mut()
        }
    }
}
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_restore(
    mut env: JNIEnv,
    _: JClass,
    data: JByteArray,
    battery: jboolean,
) {
    let bytes = match env.convert_byte_array(&data) {
        Ok(b) => b,
        Err(e) => {
            error(&mut env, e);
            return;
        }
    };
    let mut machine = MACHINE.lock().unwrap();
    let Some(nes) = machine.as_mut() else {
        error(&mut env, "No game is open");
        return;
    };
    let result = if battery != 0 {
        nes.load_battery_ram(&bytes)
    } else {
        nes.load_state(&bytes)
    };
    match result {
        // The chain led back from a different moment; keeping it would rewind
        // into a timeline the player did not play.
        Ok(()) => rewind_reset(nes),
        Err(e) => error(&mut env, e),
    }
}
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_audio(
    mut env: JNIEnv,
    _: JClass,
    playing: jboolean,
) {
    #[cfg(target_os = "android")]
    {
        // The queue target is measured in frames of audio, so the region's frame
        // rate has to be known before the first burst is asked for.
        if playing != 0 {
            if let Some(nes) = MACHINE.lock().unwrap().as_ref() {
                audio::set_frame_rate(nes.bus.cart.header.region.frame_rate() as f32);
            }
        }
        if let Err(e) = audio::set_playing(playing != 0) {
            error(&mut env, e);
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = (&mut env, playing);
}

/// Where the audio latency actually is, in milliseconds, plus the underrun count.
///
/// Four floats: the queue, the device ring, the two added together, and the
/// target the controller settled on. Underruns are the fifth. The plan asks for
/// a measurable audio figure rather than a claim, and this is it.
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_audioStats(
    env: JNIEnv,
    _: JClass,
) -> jfloatArray {
    #[cfg(target_os = "android")]
    let (queue, buffer, underruns, target) = {
        let (queue, buffer, underruns) = audio::stats();
        (queue, buffer, underruns, audio::target_ms())
    };
    #[cfg(not(target_os = "android"))]
    let (queue, buffer, underruns, target) = (0.0f32, 0.0f32, 0u32, 0.0f32);
    let values = [queue, buffer, queue + buffer, target, underruns as f32];
    match env.new_float_array(values.len() as i32) {
        Ok(array) => {
            let _ = env.set_float_array_region(&array, 0, &values);
            array.into_raw()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_frameRate(_: JNIEnv, _: JClass) -> jfloat {
    MACHINE.lock().unwrap().as_ref().map_or(60.0988, |nes| nes.bus.cart.header.region.frame_rate() as f32)
}
