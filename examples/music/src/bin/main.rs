#![no_std]
#![no_main]
#![cfg_attr(feature = "is-lp-core", feature(alloc_error_handler))]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]


use {
    esp_alloc as _,
    esp_hal::rtc_cntl::Rtc,
    esp_rs_copro_procmacro::{define_lp_allocator, load_lp_code2},
    esp_println::{print, println}
};

#[cfg(all(feature="esp32c6", feature="has-lp-core"))]
use esp_hal::lp_core::LpCore;
#[cfg(all(feature = "esp32s3", feature="has-lp-core"))]
use esp_hal::ulp_core::UlpCore;

use esp_rs_copro::{lpbox::LPBox, prelude::*};
use esp_rs_copro::{io::gpio::LPOutput, collections::lpvec::LPVec};
use core::option::Option;
use mtukai_projgen::LpContext;

use {
    esp_lp_hal::{
        delay::Delay,
        prelude::*
    },
    panic_halt as _
};

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
#[cfg(feature = "has-lp-core")]
esp_bootloader_esp_idf::esp_app_desc!();
#[cfg(feature = "has-lp-core")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(feature = "has-lp-core")]
define_lp_allocator!();

#[derive(Clone, Copy, esp_rs_copro_procmacro::MovableObject)]
pub struct Note {
    pub frequency : u16,
    pub duration : u16
}

impl Note {
    pub fn new(frequency : u16, duration : u16) -> Self {
        Note {
            frequency,
            duration
        }
    }
    pub fn rest(duration : u16) -> Self {
        Note::new(0, duration)
    }
    pub fn c4(duration : u16) -> Self {
        Note::new(261, duration)
    }
    pub fn d4(duration : u16) -> Self {
        Note::new(293, duration)
    }
    pub fn e4(duration : u16) -> Self {
        Note::new(329, duration)
    }
    pub fn f4(duration : u16) -> Self {
        Note::new(349, duration)
    }
    pub fn g4(duration : u16) -> Self {
        Note::new(392, duration)
    }
    pub fn a4(duration : u16) -> Self {
        Note::new(440, duration)
    }
    pub fn b4(duration : u16) -> Self {
        Note::new(493, duration)
    }
}


fn cde() -> LPVec<Note> {
    let mut ret = LPVec::new();
    for i in 0..8 {
        ret.push(Note::c4(5000));
        ret.push(Note::rest(5000));
    }
    ret
}

#[inline(always)]
fn delay_us(us: u32) {
    Delay.delay_micros(us - 64);
}

fn play_note<const PIN: u8>(outpin : &mut LPOutput<PIN>, note : &Note) {
    let d = (note.duration as u32) * 1000;
    if note.frequency == 0 {
        delay_us(d);
    } else {
        let period = 1000000 / note.frequency as u32;
        for _ in 0..(d / period) {
            outpin.set_level(true);
            delay_us(period / 2);
            outpin.set_level(false);
            delay_us(period / 2);
        }
    }
}

#[mtukai_projgen_procmacro::entry(4096)]
fn lpmain<'a>(_ : &mut LpContext, score: &LPVec<Note>, outpin: &mut LPOutput<'a, 1>){
    loop{
        score.iter().for_each(|note| play_note(outpin, note));
    }
}

#[cfg(feature = "has-lp-core")]
#[esp_hal::main]
fn main() -> ! {
    // generator version: 0.5.0
    esp_alloc::heap_allocator!(size: 72 * 1024);
    esp_println::logger::init_logger_from_env();
    let peripherals = esp_hal::init(esp_hal::Config::default());
        
    #[cfg(feature = "esp32c6")]
    let mut lp_core = LpCore::new(peripherals.LP_CORE);
    #[cfg(feature = "esp32s3")]
    let mut lp_core = UlpCore::new(peripherals.ULP_RISCV_CORE);
    lp_core.stop();
    println!("lp core stopped");

    // load code to LP core
    {
        let mut rtc = Rtc::new(peripherals.LPWR);
        let mut outpin = LPOutput::<1>::new(peripherals.GPIO1);
        let mut lp_context = LpContext::new(&mut lp_core, &mut rtc);
        loop{
            let score = cde();
            match lpmain(&mut lp_context, &score, &mut outpin) {
                Ok(_) => {}
                Err(e) => println!("Error running LP core: {}", e)
            }
        }
    }
    loop {}
}