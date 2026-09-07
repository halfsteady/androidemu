//! Android JNI boundary. The machine has one owner and never runs in the audio callback.
use jni::{
    objects::{JByteArray, JByteBuffer, JClass},
    sys::{jboolean, jbyteArray, jfloat, jint, jstring},
    JNIEnv,
};
use nes_core::{Buttons, Nes};
use std::sync::Mutex;
static MACHINE: Mutex<Option<Nes>> = Mutex::new(None);
#[cfg(target_os = "android")]
mod audio;
fn error(env: &mut JNIEnv, message: impl ToString) {
    let _ = env.throw_new("java/lang/IllegalStateException", message.to_string());
}
const PALETTE: [u32; 64] = [
    0x666666, 0x002a88, 0x1412a7, 0x3b00a4, 0x5c007e, 0x6e0040, 0x6c0600, 0x561d00, 0x333500,
    0x0b4800, 0x005200, 0x004f08, 0x00404d, 0, 0, 0, 0xadadad, 0x155fd9, 0x4240ff, 0x7527fe,
    0xa01acc, 0xb71e7b, 0xb53120, 0x994e00, 0x6b6d00, 0x388700, 0x0c9300, 0x008f32, 0x007c8d, 0, 0,
    0, 0xfffeff, 0x64b0ff, 0x9290ff, 0xc676ff, 0xf36aff, 0xfe6ecc, 0xfe8170, 0xea9e22, 0xbcbe00,
    0x88d800, 0x5ce430, 0x45e082, 0x48cdde, 0x4f4f4f, 0, 0, 0xfffeff, 0xc0dfff, 0xd3d2ff, 0xe8c8ff,
    0xfbc2ff, 0xfec4ea, 0xfeccc5, 0xf7d8a5, 0xe4e594, 0xcfef96, 0xbdf4ab, 0xb3f3cc, 0xb5ebf2,
    0xb8b8b8, 0, 0,
];
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
        #[cfg(target_os = "android")]
        audio::push(nes.bus.apu.samples());
    }
    // JNI verified the direct buffer capacity; Kotlin retains it for this call.
    let out = unsafe { std::slice::from_raw_parts_mut(address, 256 * 240 * 4) };
    for (pixel, &index) in out.as_chunks_mut::<4>().0.iter_mut().zip(nes.framebuffer()) {
        let rgb = PALETTE[(index & 63) as usize];
        pixel.copy_from_slice(&[(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255]);
    }
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
    if let Err(e) = result {
        error(&mut env, e);
    }
}
#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_audio(
    mut env: JNIEnv,
    _: JClass,
    playing: jboolean,
) {
    #[cfg(target_os = "android")]
    if let Err(e) = audio::set_playing(playing != 0) {
        error(&mut env, e);
    }
    #[cfg(not(target_os = "android"))]
    let _ = (&mut env, playing);
}

#[no_mangle]
pub extern "system" fn Java_dev_androidemu_Native_frameRate(_: JNIEnv, _: JClass) -> jfloat {
    MACHINE.lock().unwrap().as_ref().map_or(60.0988, |nes| nes.bus.cart.header.region.frame_rate() as f32)
}
